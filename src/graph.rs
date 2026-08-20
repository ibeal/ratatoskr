//! Reverse edges. `outline` and `show` only descend; the links *between* documents make the real
//! structure a graph, and without reverse edges you can only go down — which is the thing that makes
//! flat markdown feel flat.
//!
//! Edges come from prose: markdown inline links, reference definitions, and `@path` transclusions.
//! Frontmatter-declared edges would miss nearly all of the real structure, because most of it lives
//! in sentences like "read `workflow/sdlc.md` first".

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::errors::Result;
use crate::frontmatter;
use crate::refs::{self, Ref, RefSpace};
use crate::resolve::ResolvedManifest;

/// The whole link graph over the resolved context and stores.
#[derive(Debug, Serialize)]
pub struct ContextGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<Edge>,
    /// Links that look like a local markdown file but resolve to nothing — the graph doubles as a
    /// dead-link check.
    pub broken: Vec<BrokenEdge>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GraphNode {
    #[serde(rename = "ref")]
    pub reference: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
pub struct Edge {
    pub from: String,
    /// The canonical ref of the linked file.
    pub to: String,
    /// The `#fragment` on the link, if it addressed a heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragment: Option<String>,
    pub line: usize,
    /// The line the link was written on, so a caller is readable without opening the file.
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct BrokenEdge {
    pub from: String,
    pub target: String,
    pub line: usize,
}

impl ContextGraph {
    pub fn build(space: &RefSpace) -> Self {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut broken = Vec::new();

        for (reference, path) in space.nodes() {
            nodes.push(GraphNode {
                reference: reference.to_string(),
                path: path.to_path_buf(),
            });

            // An unreadable file contributes no edges; it is already reported elsewhere.
            let Ok(contents) = fs::read_to_string(path) else {
                continue;
            };
            let (_, body) = frontmatter::parse(&contents);

            for link in extract_links(body) {
                match resolve_link(space, path, &link.target) {
                    LinkTarget::Node(to) => edges.push(Edge {
                        from: reference.to_string(),
                        to,
                        fragment: link.fragment,
                        line: link.line,
                        text: link.text,
                    }),
                    // A target that exists on disk but is outside the ref space is not a dead
                    // link — it just is not addressable. Reporting it would make `doctor` cry
                    // wolf about every file that is deliberately not in a store or an include.
                    LinkTarget::Dangling if link.explicit && looks_local_markdown(&link.target) => {
                        broken.push(BrokenEdge {
                            from: reference.to_string(),
                            target: link.target,
                            line: link.line,
                        })
                    }
                    LinkTarget::Dangling | LinkTarget::Outside | LinkTarget::External => {}
                }
            }
        }

        // Deterministic regardless of filesystem order.
        nodes.sort_by(|left, right| left.reference.cmp(&right.reference));
        edges.sort_by(|left, right| {
            (&left.from, left.line, &left.to).cmp(&(&right.from, right.line, &right.to))
        });
        // One line naming the same target twice (a code span *and* a link) is one reference.
        edges.dedup_by(|later, first| {
            (&later.from, later.line, &later.to) == (&first.from, first.line, &first.to)
        });
        broken.sort_by(|left, right| (&left.from, left.line).cmp(&(&right.from, right.line)));

        Self {
            nodes,
            edges,
            broken,
        }
    }

    /// Every node that links to `file`. One hop: "find references" is a one-hop question, and a
    /// transitive answer over a densely cross-linked corpus is closer to "everything".
    ///
    /// `file` must be the *canonical* ref. Edges are stored canonically, so comparing against the
    /// spelling a user typed would report a confident "nothing links to this" for every alias.
    pub fn callers(&self, file: &str, heading: &[String]) -> Vec<&Edge> {
        let wanted_heading = heading.join("/");

        self.edges
            .iter()
            .filter(|edge| edge.to == file)
            .filter(|edge| {
                // A ref naming a heading only matches links that reached that heading. A ref naming
                // a file matches every link into it, fragment or not.
                wanted_heading.is_empty()
                    || edge.fragment.as_deref().is_some_and(|fragment| {
                        crate::headings::slugify(fragment)
                            == crate::headings::slugify(&wanted_heading)
                    })
            })
            .collect()
    }

    /// Forward reachability from one node, for rendering a subgraph.
    ///
    /// Cycles and multiple parents are designed in: a visited set makes a cycle terminate rather
    /// than recurse, and a node reached by two parents is simply reached twice.
    fn reachable(&self, from: &str, depth: Option<usize>) -> BTreeSet<String> {
        let mut adjacency = BTreeMap::<&str, Vec<&str>>::new();
        for edge in &self.edges {
            adjacency
                .entry(edge.from.as_str())
                .or_default()
                .push(edge.to.as_str());
        }

        let mut seen = BTreeSet::new();
        seen.insert(from.to_string());
        let mut frontier = vec![from.to_string()];
        let mut level = 0;

        while !frontier.is_empty() && depth.is_none_or(|limit| level < limit) {
            let mut next = Vec::new();
            for node in &frontier {
                for target in adjacency.get(node.as_str()).into_iter().flatten() {
                    // The visited set is what makes a cycle terminate.
                    if seen.insert((*target).to_string()) {
                        next.push((*target).to_string());
                    }
                }
            }
            frontier = next;
            level += 1;
        }

        seen
    }
}

/// A link as written, before resolution.
struct Link {
    target: String,
    fragment: Option<String>,
    line: usize,
    text: String,
    /// True for a real link or `@`-import; false for a path merely named in a code span. Only an
    /// explicit link can be a *broken* edge — a mentioned path may legitimately not exist here.
    explicit: bool,
}

/// Pull every link target out of a body, skipping fenced blocks.
///
/// Links inside a fence are examples or quoted material, not structure — counting them would make
/// every code sample a graph edge.
fn extract_links(body: &str) -> Vec<Link> {
    let mut links = Vec::new();
    let mut fenced = false;

    for (index, line) in body.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }

        for (target, explicit) in line_targets(line) {
            let (target, fragment) = split_fragment(&target);
            if target.is_empty() {
                // A bare `#anchor` is an intra-file link; there is no file edge to record.
                continue;
            }
            links.push(Link {
                target,
                fragment,
                line: index + 1,
                text: line.trim().to_string(),
                explicit,
            });
        }
    }

    links
}

/// Every link target on one line, paired with whether it was written as an explicit link.
fn line_targets(line: &str) -> Vec<(String, bool)> {
    let mut targets = Vec::new();
    let bytes: Vec<char> = line.chars().collect();

    // `[label]: target` — a reference definition.
    if let Some(rest) = reference_definition(line) {
        targets.push((rest.to_string(), true));
    }

    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            // `[text](target)` — an inline link. Link text may itself contain brackets (a linked
            // image, `[![alt](pic.png)](target)`, or a nested label), so the matching `]` is found
            // by depth rather than by the first one.
            '[' if is_link_open(&bytes, index) => {
                let Some(close) = matching_bracket(&bytes, index) else {
                    // An unbalanced `[` is ordinary prose; keep scanning the rest of the line
                    // rather than abandoning every link after it.
                    index += 1;
                    continue;
                };
                let end = (bytes.get(close + 1) == Some(&'('))
                    .then(|| find(&bytes, close + 2, ')'))
                    .flatten();
                if let Some(end) = end {
                    let target: String = bytes[close + 2..end].iter().collect();
                    // A link title (`(url "title")`) is not part of the target.
                    if let Some(target) = target.split_whitespace().next() {
                        targets.push((target.to_string(), true));
                    }
                    // Resume *inside* the link text, so a nested link is found too.
                    index += 1;
                    continue;
                }
                index += 1;
            }
            // `@path` — a transclusion. Only at the start of a line or after whitespace, so an
            // email address in prose is not an edge.
            '@' if index == 0 || bytes[index - 1].is_whitespace() => {
                let end = bytes[index + 1..]
                    .iter()
                    .position(|c| c.is_whitespace())
                    .map(|offset| index + 1 + offset)
                    .unwrap_or(bytes.len());
                let target: String = bytes[index + 1..end].iter().collect();
                if !target.is_empty() && !target.contains('@') {
                    targets.push((target, true));
                }
                index = end;
            }
            // `` `path/to/file.md` `` — a path named in a sentence. Most of the real structure is
            // written this way ("read `workflow/sdlc.md` first"), so ignoring code spans would
            // miss the majority of the graph. Held to a tight shape to stay low-noise, and never
            // counted as a *broken* edge, since a mentioned path may legitimately live elsewhere.
            '`' => {
                let Some(close) = find(&bytes, index + 1, '`') else {
                    // An unmatched backtick is prose; the rest of the line still holds links.
                    index += 1;
                    continue;
                };
                let span: String = bytes[index + 1..close].iter().collect();
                if is_path_like(&span) {
                    targets.push((span, false));
                }
                index = close + 1;
            }
            _ => index += 1,
        }
    }

    targets
}

/// A code span that is plausibly a path to a markdown file in this corpus: no whitespace, no glob,
/// and a `.md` extension.
fn is_path_like(span: &str) -> bool {
    !span.is_empty()
        && !span.contains(char::is_whitespace)
        && !span.contains(['*', '?', '<', '>'])
        && span.to_ascii_lowercase().ends_with(".md")
}

fn reference_definition(line: &str) -> Option<&str> {
    let rest = line.trim().strip_prefix('[')?;
    let (_, rest) = rest.split_once("]:")?;
    let target = rest.split_whitespace().next()?;
    Some(target)
}

/// The `]` that closes the `[` at `open`, counting nested pairs.
fn matching_bracket(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for index in open..chars.len() {
        match chars[index] {
            '[' if !is_escaped(chars, index) => depth += 1,
            ']' if !is_escaped(chars, index) => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// A `[` opens a link unless it is escaped, or preceded by an unescaped `!` — which makes it an
/// image, not a link to follow.
fn is_link_open(chars: &[char], index: usize) -> bool {
    if is_escaped(chars, index) {
        return false;
    }
    let image = index > 0 && chars[index - 1] == '!' && !is_escaped(chars, index - 1);
    !image
}

/// A backslash-escaped bracket is literal text, not link syntax.
fn is_escaped(chars: &[char], index: usize) -> bool {
    let mut slashes = 0;
    while index > slashes && chars[index - 1 - slashes] == '\\' {
        slashes += 1;
    }
    slashes % 2 == 1
}

fn find(chars: &[char], from: usize, wanted: char) -> Option<usize> {
    chars[from..]
        .iter()
        .position(|c| *c == wanted)
        .map(|offset| from + offset)
}

fn split_fragment(target: &str) -> (String, Option<String>) {
    // Strip a trailing markdown-emphasis or punctuation artefact the writer's prose left attached.
    let target = target.trim_end_matches(['.', ',', ')', '`', '*']);
    match target.split_once('#') {
        Some((path, fragment)) if !fragment.is_empty() => {
            (path.to_string(), Some(fragment.to_string()))
        }
        Some((path, _)) => (path.to_string(), None),
        None => (target.to_string(), None),
    }
}

/// What a written link target turned out to be.
enum LinkTarget {
    /// An addressable node, by canonical ref.
    Node(String),
    /// A real file that is simply not in the ref space.
    Outside,
    /// Points at nothing on disk.
    Dangling,
    /// A URL or an intra-file anchor.
    External,
}

/// Resolve a written link target.
///
/// Two spellings both work: a path relative to the linking file (how markdown links are written)
/// and a scope-relative ref (how `rata.toml` and prose code spans name things). Trying both is what
/// lets `` `context/PREFERENCES.md` `` in a table resolve from a file two directories down.
fn resolve_link(space: &RefSpace, source: &Path, target: &str) -> LinkTarget {
    if is_external(target) {
        return LinkTarget::External;
    }

    let Some(parent) = source.parent() else {
        return LinkTarget::External;
    };
    let joined = normalize(&if target.starts_with('/') {
        PathBuf::from(target)
    } else {
        parent.join(target)
    });

    if let Some(reference) = space.canonical_ref(&joined) {
        return LinkTarget::Node(reference.to_string());
    }
    if let Some(reference) = space.lookup_ref(target) {
        return LinkTarget::Node(reference.to_string());
    }
    if joined.exists() {
        return LinkTarget::Outside;
    }
    LinkTarget::Dangling
}

fn is_external(target: &str) -> bool {
    target.contains("://")
        || target.starts_with("mailto:")
        || target.starts_with('#')
        || target.is_empty()
}

/// Only flag a dead link when the target really looks like a local markdown file. A link to a
/// directory, a non-markdown file, or a store ref written as prose is not a broken edge.
fn looks_local_markdown(target: &str) -> bool {
    !is_external(target) && target.to_ascii_lowercase().ends_with(".md")
}

/// Resolve `.` and `..` textually. The real path may not exist, and `canonicalize` would fail on a
/// dead link — which is exactly the case that has to be reportable.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[derive(Debug, Serialize)]
pub struct CallersReport {
    #[serde(rename = "ref")]
    pub reference: String,
    pub callers: Vec<Caller>,
}

#[derive(Debug, Serialize)]
pub struct Caller {
    pub from: String,
    pub line: usize,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragment: Option<String>,
}

pub fn build_callers(
    cwd: &Path,
    global_root_override: Option<&Path>,
    selected_profiles: &[String],
    reference: &str,
) -> Result<CallersReport> {
    let (space, _) = refs::ref_space(cwd, global_root_override, selected_profiles)?;
    let parsed = Ref::parse(reference);
    // Resolve first, so a typo fails with candidates rather than reporting "no callers".
    let resolved = space.resolve(&parsed)?;
    // Then canonicalize: a file may be addressable several ways (a store node that is also a
    // context include, an absolute path, a unique suffix), and edges only carry one of them.
    let canonical = space
        .lookup_ref(&parsed.file_address())
        .unwrap_or(&resolved.reference)
        .to_string();
    let graph = ContextGraph::build(&space);

    Ok(CallersReport {
        reference: heading_suffixed(&canonical, &parsed.heading),
        callers: graph
            .callers(&canonical, &parsed.heading)
            .into_iter()
            .map(|edge| Caller {
                from: edge.from.clone(),
                line: edge.line,
                text: edge.text.clone(),
                fragment: edge.fragment.clone(),
            })
            .collect(),
    })
}

fn heading_suffixed(file: &str, heading: &[String]) -> String {
    if heading.is_empty() {
        return file.to_string();
    }
    format!("{file}#{}", heading.join("/"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphFormat {
    Mermaid,
    Dot,
}

#[derive(Debug, Serialize)]
pub struct GraphReport {
    #[serde(skip_serializing)]
    pub format: GraphFormat,
    pub from: Option<String>,
    pub depth: Option<usize>,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<Edge>,
}

pub fn build_graph(
    cwd: &Path,
    global_root_override: Option<&Path>,
    selected_profiles: &[String],
    format: GraphFormat,
    from: Option<&str>,
    depth: Option<usize>,
) -> Result<GraphReport> {
    let (space, _) = refs::ref_space(cwd, global_root_override, selected_profiles)?;
    let graph = ContextGraph::build(&space);

    let (nodes, edges) = match from {
        None => (graph.nodes.clone(), graph.edges.clone()),
        Some(from) => {
            let parsed = Ref::parse(from);
            space.resolve(&parsed)?;
            let root = space
                .lookup_ref(&parsed.file_address())
                .unwrap_or(&parsed.target)
                .to_string();
            let included = graph.reachable(&root, depth);
            (
                graph
                    .nodes
                    .iter()
                    .filter(|node| included.contains(&node.reference))
                    .cloned()
                    .collect(),
                graph
                    .edges
                    .iter()
                    .filter(|edge| included.contains(&edge.from) && included.contains(&edge.to))
                    .cloned()
                    .collect(),
            )
        }
    };

    Ok(GraphReport {
        format,
        from: from.map(str::to_string),
        depth,
        nodes,
        edges,
    })
}

impl Display for CallersReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "# Ratatoskr Callers")?;
        writeln!(f)?;
        writeln!(f, "ref: {}", self.reference)?;
        writeln!(f, "callers: {}", self.callers.len())?;
        writeln!(f)?;
        if self.callers.is_empty() {
            return writeln!(f, "nothing links to this");
        }
        for caller in &self.callers {
            writeln!(f, "- {}:{}", caller.from, caller.line)?;
            writeln!(f, "  {}", caller.text)?;
        }
        Ok(())
    }
}

impl Display for GraphReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.format {
            GraphFormat::Mermaid => write_mermaid(f, self),
            GraphFormat::Dot => write_dot(f, self),
        }
    }
}

/// Rendering only — no layout opinions.
fn write_mermaid(f: &mut fmt::Formatter<'_>, report: &GraphReport) -> fmt::Result {
    writeln!(f, "graph LR")?;
    for node in &report.nodes {
        writeln!(f, "  {}[\"{}\"]", node_id(&node.reference), node.reference)?;
    }
    for edge in dedup_edges(&report.edges) {
        writeln!(f, "  {} --> {}", node_id(&edge.0), node_id(&edge.1))?;
    }
    Ok(())
}

fn write_dot(f: &mut fmt::Formatter<'_>, report: &GraphReport) -> fmt::Result {
    writeln!(f, "digraph context {{")?;
    for node in &report.nodes {
        writeln!(
            f,
            "  {} [label=\"{}\"];",
            node_id(&node.reference),
            node.reference
        )?;
    }
    for edge in dedup_edges(&report.edges) {
        writeln!(f, "  {} -> {};", node_id(&edge.0), node_id(&edge.1))?;
    }
    writeln!(f, "}}")
}

/// One arrow per pair, however many times the link is written.
fn dedup_edges(edges: &[Edge]) -> BTreeSet<(String, String)> {
    edges
        .iter()
        .map(|edge| (edge.from.clone(), edge.to.clone()))
        .collect()
}

/// A diagram-safe identifier. Sanitizing alone would collide (`a-b.md` and `a_b.md` both become
/// `a_b_md`), so a short hash of the real ref is appended.
fn node_id(reference: &str) -> String {
    let sanitized = reference
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in reference.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("n_{sanitized}_{:x}", hash & 0xffff_ffff)
}

/// Broken edges, for `doctor`.
pub fn broken_edges(manifest: &ResolvedManifest) -> Result<Vec<BrokenEdge>> {
    let space = RefSpace::build(manifest)?;
    Ok(ContextGraph::build(&space).broken)
}

impl From<crate::cli::GraphSyntax> for GraphFormat {
    fn from(value: crate::cli::GraphSyntax) -> Self {
        match value {
            crate::cli::GraphSyntax::Mermaid => Self::Mermaid,
            crate::cli::GraphSyntax::Dot => Self::Dot,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{
        GraphFormat, build_callers, build_graph, extract_links, looks_local_markdown, normalize,
    };
    use crate::test_support::temp_dir;

    fn fixture(label: &str) -> (PathBuf, PathBuf) {
        let root = temp_dir(label);
        let global_root = root.join("global");
        fs::create_dir_all(global_root.join("workflow")).unwrap();
        fs::create_dir_all(global_root.join("context")).unwrap();
        fs::create_dir_all(global_root.join("memory")).unwrap();
        fs::write(
            global_root.join("rata.toml"),
            "version = 1\n\n[context]\ninclude = [\"AGENTS.md\", \"context/PREFERENCES.md\", \"workflow/sdlc.md\"]\n\n[stores]\nmemory = \"memory\"\n",
        )
        .unwrap();
        fs::write(
            global_root.join("AGENTS.md"),
            "# Agents\n\nSee [preferences](context/PREFERENCES.md) and @workflow/sdlc.md.\n\n\
             ```sh\n# [not a link](context/NOPE.md)\n```\n\nAlso [gone](context/MISSING.md).\n",
        )
        .unwrap();
        fs::write(
            global_root.join("workflow/sdlc.md"),
            "# Workflow\n\nStyle comes from [prefs](../context/PREFERENCES.md#Response-style).\n\n\
             ## Loop\n\nBack to [agents](../AGENTS.md).\n",
        )
        .unwrap();
        fs::write(
            global_root.join("context/PREFERENCES.md"),
            "# Preferences\n\nHow Ian likes things.\n\n## Response style\n\nBe concise.\n",
        )
        .unwrap();
        fs::write(global_root.join("memory/note.md"), "# Note\n\nA memory.\n").unwrap();
        (root, global_root)
    }

    #[test]
    fn callers_finds_prose_links_and_at_imports() {
        let (root, global_root) = fixture("graph-callers");

        let report =
            build_callers(&root, Some(&global_root), &[], "context/PREFERENCES.md").unwrap();
        let sources = report
            .callers
            .iter()
            .map(|caller| caller.from.as_str())
            .collect::<Vec<_>>();
        assert_eq!(sources, vec!["AGENTS.md", "workflow/sdlc.md"]);
        // The linking line comes back, so a caller is readable without opening the file.
        assert!(report.callers[0].text.contains("[preferences]"));

        // The @-import is an edge too.
        let sdlc = build_callers(&root, Some(&global_root), &[], "workflow/sdlc.md").unwrap();
        assert_eq!(sdlc.callers.len(), 1);
        assert_eq!(sdlc.callers[0].from, "AGENTS.md");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_heading_ref_only_matches_links_that_reached_that_heading() {
        let (root, global_root) = fixture("graph-heading-callers");

        let heading = build_callers(
            &root,
            Some(&global_root),
            &[],
            "context/PREFERENCES.md#Response style",
        )
        .unwrap();
        assert_eq!(heading.callers.len(), 1);
        assert_eq!(heading.callers[0].from, "workflow/sdlc.md");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn nested_brackets_and_unbalanced_delimiters_do_not_lose_links() {
        let cases: &[(&str, Vec<&str>)] = &[
            // A linked image: the first `]` belongs to the image, not the link.
            // The image is skipped; only the outer link is an edge.
            ("[![img](pic.png)](T.md)", vec!["T.md"]),
            // A nested label.
            ("[nested [inner] label](T.md)", vec!["T.md"]),
            // An unmatched backtick must not abandon the rest of the line.
            ("`unclosed and then [link](T.md)", vec!["T.md"]),
            // Nor an unbalanced bracket.
            ("[stray and then [x](T.md)", vec!["T.md"]),
            // An image on its own is not a navigational edge.
            ("![alt](pic.md)", vec![]),
            // An escaped bracket is literal text.
            ("\\[escaped\\](T.md)", vec![]),
        ];

        for (input, expected) in cases {
            let found = extract_links(input)
                .iter()
                .map(|link| link.target.clone())
                .collect::<Vec<_>>();
            assert_eq!(&found, expected, "input: {input}");
        }
    }

    #[test]
    fn one_line_naming_a_target_twice_is_one_edge() {
        let (root, global_root) = fixture("graph-dedup");
        // A code span and an explicit link to the same file, on one line.
        fs::write(
            global_root.join("AGENTS.md"),
            "# Agents\n\nSee `context/PREFERENCES.md` at [prefs](context/PREFERENCES.md).\n",
        )
        .unwrap();

        let report =
            build_callers(&root, Some(&global_root), &[], "context/PREFERENCES.md").unwrap();
        let from_agents = report
            .callers
            .iter()
            .filter(|caller| caller.from == "AGENTS.md")
            .count();
        assert_eq!(from_agents, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn callers_accepts_any_valid_spelling_of_a_ref() {
        let (root, global_root) = fixture("graph-aliases");
        // memory:note is reachable by store ref; PREFERENCES by relative path, absolute path,
        // and unique suffix. Every spelling must give the same answer.
        let absolute = global_root.join("context/PREFERENCES.md");
        for reference in [
            "context/PREFERENCES.md",
            "PREFERENCES.md",
            absolute.to_str().unwrap(),
        ] {
            let report = build_callers(&root, Some(&global_root), &[], reference).unwrap();
            assert_eq!(report.callers.len(), 2, "spelling: {reference}");
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn links_inside_a_fence_are_not_edges() {
        let links = extract_links("[real](a.md)\n\n```\n[fake](b.md)\n```\n\n[also real](c.md)\n");
        let targets = links
            .iter()
            .map(|link| link.target.as_str())
            .collect::<Vec<_>>();
        assert_eq!(targets, vec!["a.md", "c.md"]);
    }

    #[test]
    fn reference_definitions_and_titles_are_handled() {
        let links = extract_links("[label]: workflow/sdlc.md\n\n[t](a.md \"Title\")\n");
        let targets = links
            .iter()
            .map(|link| link.target.as_str())
            .collect::<Vec<_>>();
        assert_eq!(targets, vec!["workflow/sdlc.md", "a.md"]);
    }

    #[test]
    fn an_email_address_is_not_a_transclusion() {
        let links = extract_links("Mail ian@example.com or see @workflow/sdlc.md.\n");
        let targets = links
            .iter()
            .map(|link| link.target.as_str())
            .collect::<Vec<_>>();
        assert_eq!(targets, vec!["workflow/sdlc.md"]);
    }

    #[test]
    fn a_cycle_terminates_and_a_graph_renders() {
        let (root, global_root) = fixture("graph-cycle");
        // AGENTS.md -> sdlc.md -> AGENTS.md is already a cycle in the fixture.

        let mermaid = build_graph(
            &root,
            Some(&global_root),
            &[],
            GraphFormat::Mermaid,
            Some("AGENTS.md"),
            None,
        )
        .unwrap();
        let rendered = mermaid.to_string();
        assert!(rendered.starts_with("graph LR"));
        assert!(rendered.contains("--> "));

        // Depth 1 reaches only the direct targets.
        let shallow = build_graph(
            &root,
            Some(&global_root),
            &[],
            GraphFormat::Dot,
            Some("AGENTS.md"),
            Some(1),
        )
        .unwrap();
        let names = shallow
            .nodes
            .iter()
            .map(|node| node.reference.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec!["AGENTS.md", "context/PREFERENCES.md", "workflow/sdlc.md"]
        );
        assert!(shallow.to_string().starts_with("digraph context {"));

        // The memory node is unreachable from AGENTS.md and correctly excluded.
        assert!(!names.contains(&"memory:note"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_dead_link_is_reported_as_a_broken_edge() {
        let (root, global_root) = fixture("graph-broken");
        let manifest = crate::resolve::resolve_manifest(&root, Some(&global_root), &[]).unwrap();

        let broken = super::broken_edges(&manifest).unwrap();
        assert_eq!(broken.len(), 1, "{broken:?}");
        assert_eq!(broken[0].from, "AGENTS.md");
        assert_eq!(broken[0].target, "context/MISSING.md");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_local_markdown_targets_count_as_dead_links() {
        assert!(looks_local_markdown("context/PREFERENCES.md"));
        assert!(!looks_local_markdown("https://example.com/a.md"));
        assert!(!looks_local_markdown("../scripts/run.sh"));
        assert!(!looks_local_markdown("#a-heading"));
    }

    #[test]
    fn parent_segments_resolve_without_touching_the_filesystem() {
        assert_eq!(
            normalize(Path::new("/a/b/workflow/../context/x.md")),
            PathBuf::from("/a/b/context/x.md")
        );
    }
}
