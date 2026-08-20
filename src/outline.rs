use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::errors::{RatatoskrError, Result};
use crate::frontmatter::{self, Frontmatter};
use crate::resolve::{self, ResolvedStore};

/// How long a rendered signature may get before the text output truncates it. The ladder itself
/// keeps the full sentence; only presentation is capped.
const SIGNATURE_RENDER_LIMIT: usize = 120;

#[derive(Debug, Serialize)]
pub struct OutlineReport {
    pub cwd: PathBuf,
    pub depth: Option<usize>,
    pub stores: Vec<OutlineStore>,
}

#[derive(Debug, Serialize)]
pub struct OutlineStore {
    pub name: String,
    pub paths: Vec<PathBuf>,
    pub nodes: Vec<Node>,
    /// Directories or filenames the scan could not use. Reported, never fatal.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub scan_issues: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Node {
    /// Store-relative address without the `.md` extension, e.g. `nix/patterns`.
    #[serde(rename = "ref")]
    pub reference: String,
    pub store: String,
    pub path: PathBuf,
    /// Path segments below the store root; 1 for a file directly in the store.
    pub depth: usize,
    pub signature: String,
    pub tier: SignatureTier,
    /// The signature says nothing the ref does not; render the ref alone.
    pub redundant: bool,
    pub tags: Vec<String>,
    /// Layers that also define this ref but lost to the one above. Recorded so a shadowed file is
    /// visible rather than mysteriously absent.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shadowed: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<frontmatter::FrontmatterIssue>,
    /// Why the file could not be read, when it could not. The node still exists so the problem is
    /// reportable instead of aborting the scan.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unreadable: Option<String>,
}

impl Node {
    /// True when this node's frontmatter tries to decide its own eagerness — a hard error.
    pub fn has_eagerness_key(&self) -> bool {
        frontmatter::has_eagerness_key(&self.issues)
    }
}

/// Which rung of the fallback ladder produced a node's signature. Lower is better.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SignatureTier {
    /// Frontmatter `description:`.
    Description,
    /// First sentence of the body after the H1.
    FirstSentence,
    /// The H1 heading text.
    Heading,
    /// Humanized filename — nothing in the file described it.
    Filename,
}

impl SignatureTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Description => "description",
            Self::FirstSentence => "first-sentence",
            Self::Heading => "heading",
            Self::Filename => "filename",
        }
    }
}

pub fn build_outline(
    cwd: &Path,
    global_root_override: Option<&Path>,
    store: Option<&str>,
    depth: Option<usize>,
) -> Result<OutlineReport> {
    let resolved = resolve::resolve_stores(cwd, global_root_override, &[])?;
    let stores = outline_stores(&resolved.stores, store, depth)?;

    Ok(OutlineReport {
        cwd: resolved.cwd,
        depth,
        stores,
    })
}

/// Scan already-resolved stores into outlines, so callers that have a manifest in hand do not
/// resolve it twice.
pub fn outline_stores(
    resolved: &BTreeMap<String, ResolvedStore>,
    store: Option<&str>,
    depth: Option<usize>,
) -> Result<Vec<OutlineStore>> {
    if let Some(name) = store.filter(|name| !resolved.contains_key(*name)) {
        return Err(RatatoskrError::UnknownStore {
            name: name.to_string(),
            available: resolved.keys().cloned().collect(),
        });
    }

    let stores = resolved
        .iter()
        .filter(|(name, _)| store.is_none_or(|wanted| wanted == name.as_str()))
        .map(|(name, resolved_store)| {
            // A scope that is both the global root and a local root contributes the same layer
            // twice, and with an intermediate scope the repeats need not be adjacent.
            let mut seen = BTreeSet::new();
            let paths = resolved_store
                .paths
                .iter()
                .filter(|path| seen.insert((*path).clone()))
                .cloned()
                .collect::<Vec<_>>();
            let (nodes, scan_issues) = collect_nodes(name, &paths, depth);
            OutlineStore {
                name: name.clone(),
                paths,
                nodes,
                scan_issues,
            }
        })
        .collect::<Vec<_>>();

    Ok(stores)
}

/// How deep a store scan will follow subdirectories. Symlinks are not followed at all, but a
/// pathological real directory tree should still terminate.
const MAX_SCAN_DEPTH: usize = 32;

/// Scan every layer of a store, in resolved precedence order, into nodes addressed by ref.
///
/// The index is computed here rather than read from a file, so there is nothing to keep in sync.
/// A ref found in more than one layer resolves to the first layer that has it, and the layers it
/// shadows are recorded rather than dropped.
///
/// Nothing here fails the whole scan. An unreadable file or directory becomes an issue on the node
/// or store that carries it, because the command most likely to meet a broken file is `doctor`, and
/// a diagnostic that dies on the thing it is meant to diagnose is useless.
fn collect_nodes(store: &str, paths: &[PathBuf], depth: Option<usize>) -> (Vec<Node>, Vec<String>) {
    let mut nodes = Vec::<Node>::new();
    let mut index = BTreeMap::<String, usize>::new();
    let mut scan_issues = Vec::new();

    for root in paths {
        let mut files = Vec::new();
        collect_markdown_files(root, 0, &mut files, &mut scan_issues);
        files.sort();

        for path in files {
            let Some(reference) = node_ref(root, &path) else {
                scan_issues.push(format!(
                    "{}: filename is not addressable as a ref",
                    path.display()
                ));
                continue;
            };
            let node_depth = reference.split('/').count();
            if depth.is_some_and(|limit| node_depth > limit) {
                continue;
            }
            // A later layer's copy of the same ref is shadowed, not silently discarded.
            if let Some(existing) = index.get(&reference) {
                nodes[*existing].shadowed.push(path);
                continue;
            }
            index.insert(reference.clone(), nodes.len());
            nodes.push(read_node(store, &reference, &path, node_depth));
        }
    }

    nodes.sort_by(|left, right| left.reference.cmp(&right.reference));
    (nodes, scan_issues)
}

fn collect_markdown_files(
    root: &Path,
    depth: usize,
    files: &mut Vec<PathBuf>,
    issues: &mut Vec<String>,
) {
    if depth >= MAX_SCAN_DEPTH {
        issues.push(format!(
            "{}: stopped scanning at {MAX_SCAN_DEPTH} levels deep",
            root.display()
        ));
        return;
    }

    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        // A declared store layer that does not exist yet simply contributes nothing.
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return,
        Err(source) => {
            issues.push(format!("{}: {source}", root.display()));
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(source) => {
                issues.push(format!("{}: {source}", root.display()));
                continue;
            }
        };
        let path = entry.path();
        if is_hidden(&path) {
            continue;
        }
        // `file_type` does not follow symlinks, so a symlinked directory is never descended into.
        // A self-referential link would otherwise generate refs until the OS ran out of levels.
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(source) => {
                issues.push(format!("{}: {source}", path.display()));
                continue;
            }
        };
        if file_type.is_dir() {
            collect_markdown_files(&path, depth + 1, files, issues);
        } else if file_type.is_file() && is_markdown(&path) {
            files.push(path);
        }
    }
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

/// A ref must round-trip through the `store:ref` and `ref#heading` addressing, so a filename
/// containing the separators has no valid ref.
fn node_ref(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let stem = relative.with_extension("");
    let text = stem.to_str()?.replace(std::path::MAIN_SEPARATOR, "/");
    let addressable = !text.is_empty() && !text.contains(':') && !text.contains('#');
    addressable.then_some(text)
}

fn read_node(store: &str, reference: &str, path: &Path, depth: usize) -> Node {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(source) => {
            return Node {
                reference: reference.to_string(),
                store: store.to_string(),
                path: path.to_path_buf(),
                depth,
                signature: format!("<unreadable: {source}>"),
                tier: SignatureTier::Filename,
                redundant: false,
                tags: Vec::new(),
                shadowed: Vec::new(),
                issues: Vec::new(),
                unreadable: Some(source.to_string()),
            };
        }
    };
    let (front, body) = frontmatter::parse(&contents);
    let (signature, tier) = resolve_signature(&front, body, reference);

    Node {
        reference: reference.to_string(),
        store: store.to_string(),
        path: path.to_path_buf(),
        depth,
        redundant: is_redundant(&signature, reference),
        signature,
        tier,
        tags: front.tags.clone(),
        shadowed: Vec::new(),
        issues: front.issues,
        unreadable: None,
    }
}

/// The fallback ladder: `description:` → first sentence after the H1 → the H1 → the filename.
/// Every node gets a signature; nothing in a file is mandatory for that to work.
pub fn resolve_signature(
    front: &Frontmatter,
    body: &str,
    reference: &str,
) -> (String, SignatureTier) {
    if let Some(description) = front
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return (description.to_string(), SignatureTier::Description);
    }

    let heading = first_heading(body);

    if let Some(sentence) = first_sentence(body, heading.is_some()) {
        return (sentence, SignatureTier::FirstSentence);
    }

    if let Some(heading) = heading {
        return (
            strip_ref_prefix(&clean_inline(heading), reference),
            SignatureTier::Heading,
        );
    }

    (humanize(reference), SignatureTier::Filename)
}

/// Headings often restate the filename before the real title (`ANGL-1 — Do the thing`). Keep the
/// part that carries information; the ref is already on the line.
fn strip_ref_prefix(signature: &str, reference: &str) -> String {
    let name = reference.rsplit('/').next().unwrap_or(reference);
    for separator in [" — ", " – ", " - ", ": "] {
        let Some((head, tail)) = signature.split_once(separator) else {
            continue;
        };
        if squash(head) == squash(name) && !tail.trim().is_empty() {
            return tail.trim().to_string();
        }
    }
    signature.to_string()
}

fn first_heading(body: &str) -> Option<&str> {
    body.lines()
        .find(|line| !line.trim().is_empty())
        .and_then(|line| line.trim().strip_prefix("# "))
        .map(str::trim)
}

/// The first sentence of the first prose paragraph. Headings, list items, quotes, tables and fenced
/// blocks are structure rather than description, so they are skipped — as are the wrapped
/// continuation lines of a skipped list item, which are still that item's text.
fn first_sentence(body: &str, skip_heading: bool) -> Option<String> {
    let mut lines = body.lines();
    if skip_heading {
        for line in lines.by_ref() {
            if line.trim().starts_with("# ") {
                break;
            }
        }
    }

    let mut paragraph = String::new();
    let mut fenced = false;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if trimmed.is_empty() {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }
        // Indented text before any prose has started is a continuation of the structure above it.
        let continuation = paragraph.is_empty() && line.starts_with(char::is_whitespace);
        if continuation || is_structural(trimmed) {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }
        if !paragraph.is_empty() {
            paragraph.push(' ');
        }
        paragraph.push_str(trimmed);
    }

    let sentence = clean_inline(&paragraph);
    let sentence = cut_at_sentence_end(&sentence);
    is_useful_sentence(sentence).then(|| sentence.to_string())
}

/// A candidate has to actually describe something. A bold lead-in label (`**Original:**`) or a
/// metadata line reduces to `Original:`, which is worse than the H1 the ladder would otherwise
/// fall through to — so a fragment that only labels is rejected.
fn is_useful_sentence(sentence: &str) -> bool {
    let mut words = sentence.split_whitespace();
    // A leading `Phase:` / `Original:` is a metadata label; what follows is a field value, not a
    // description of the file.
    let labelled = words.next().is_some_and(|first| first.ends_with(':'));
    !sentence.is_empty() && !sentence.ends_with(':') && !labelled && words.next().is_some()
}

fn is_structural(line: &str) -> bool {
    line.starts_with('#')
        || line.starts_with('>')
        || line.starts_with('|')
        || line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("+ ")
        || line.starts_with("<!--")
        || line.starts_with("@")
        || is_ordered_item(line)
        || line.chars().all(|c| c == '-' || c == '=')
}

/// `1. item` / `2) item` — a list, not a paragraph.
fn is_ordered_item(line: &str) -> bool {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    digits > 0
        && line[digits..]
            .strip_prefix(['.', ')'])
            .is_some_and(|rest| rest.starts_with(' '))
}

fn cut_at_sentence_end(text: &str) -> &str {
    let bytes = text.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if !matches!(byte, b'.' | b'!' | b'?') {
            continue;
        }
        // Abbreviations and decimals keep going; a sentence ends at whitespace or end of text.
        match bytes.get(index + 1) {
            None => return &text[..index + 1],
            Some(next) if next.is_ascii_whitespace() => return &text[..index + 1],
            Some(_) => {}
        }
    }
    text
}

/// Strip the inline markup that would make a one-line signature noisy, keeping the words.
fn clean_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '*' | '_' | '`' => {}
            '[' => {
                // `[text](url)` and `[text][ref]` both reduce to `text`.
                for inner in chars.by_ref() {
                    if inner == ']' {
                        break;
                    }
                    if !matches!(inner, '*' | '_' | '`') {
                        out.push(inner);
                    }
                }
                match chars.peek() {
                    Some('(') | Some('[') => {
                        let close = if chars.peek() == Some(&'(') { ')' } else { ']' };
                        chars.next();
                        for inner in chars.by_ref() {
                            if inner == close {
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => out.push(c),
        }
    }

    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn humanize(reference: &str) -> String {
    let name = reference.rsplit('/').next().unwrap_or(reference);
    let words = name.replace(['-', '_'], " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => words,
    }
}

/// True when the signature adds nothing to the ref — `foo-bar — Foo bar` is noise, not information.
fn is_redundant(signature: &str, reference: &str) -> bool {
    let name = reference.rsplit('/').next().unwrap_or(reference);
    squash(signature) == squash(name)
}

fn squash(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn truncate(signature: &str) -> String {
    if signature.chars().count() <= SIGNATURE_RENDER_LIMIT {
        return signature.to_string();
    }
    let cut = signature
        .char_indices()
        .nth(SIGNATURE_RENDER_LIMIT)
        .map(|(index, _)| index)
        .unwrap_or(signature.len());
    let head = signature[..cut].trim_end();
    let head = head.rsplit_once(' ').map(|(left, _)| left).unwrap_or(head);
    format!("{head}…")
}

impl Display for OutlineReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "# Ratatoskr Outline")?;
        writeln!(f)?;
        writeln!(f, "cwd: {}", self.cwd.display())?;
        writeln!(
            f,
            "depth: {}",
            self.depth
                .map(|depth| depth.to_string())
                .unwrap_or_else(|| "unlimited".to_string())
        )?;

        if self.stores.is_empty() {
            writeln!(f)?;
            return writeln!(f, "no stores resolved");
        }

        for store in &self.stores {
            writeln!(f)?;
            writeln!(f, "## {}", store.name)?;
            writeln!(f)?;
            for path in &store.paths {
                writeln!(f, "path: {}", path.display())?;
            }
            writeln!(f)?;
            if store.nodes.is_empty() {
                writeln!(f, "- <empty>")?;
                continue;
            }
            for node in &store.nodes {
                if node.redundant {
                    writeln!(f, "- {}", node.reference)?;
                } else {
                    writeln!(f, "- {} — {}", node.reference, truncate(&node.signature))?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{SignatureTier, build_outline, clean_inline, humanize, is_redundant, truncate};
    use crate::frontmatter;

    fn signature(contents: &str, reference: &str) -> (String, SignatureTier) {
        let (front, body) = frontmatter::parse(contents);
        super::resolve_signature(&front, body, reference)
    }

    #[test]
    fn ladder_prefers_the_frontmatter_description() {
        let (value, tier) = signature(
            "---\ndescription: Says it plainly\n---\n# Heading\n\nProse.\n",
            "node",
        );
        assert_eq!(value, "Says it plainly");
        assert_eq!(tier, SignatureTier::Description);
    }

    #[test]
    fn ladder_falls_back_to_the_first_sentence_after_the_h1() {
        let (value, tier) = signature(
            "# Containerized agents\n\nScope and task boundaries for boxed drivers. More text.\n",
            "containerized-agents",
        );
        assert_eq!(value, "Scope and task boundaries for boxed drivers.");
        assert_eq!(tier, SignatureTier::FirstSentence);
    }

    #[test]
    fn ladder_falls_back_to_the_heading_then_the_filename() {
        let (heading, heading_tier) = signature("# Just A Heading\n", "just-a-heading");
        assert_eq!(heading, "Just A Heading");
        assert_eq!(heading_tier, SignatureTier::Heading);

        let (name, name_tier) = signature("", "nix-patterns");
        assert_eq!(name, "Nix patterns");
        assert_eq!(name_tier, SignatureTier::Filename);
    }

    #[test]
    fn structure_is_not_mistaken_for_prose() {
        let (value, tier) = signature(
            "# Memory\n\n> A blockquote.\n\n- a pointer\n- another\n\nThe real sentence. Rest.\n",
            "memory",
        );
        assert_eq!(value, "The real sentence.");
        assert_eq!(tier, SignatureTier::FirstSentence);
    }

    #[test]
    fn fenced_blocks_are_skipped_when_looking_for_prose() {
        let (value, _) = signature(
            "# T\n\n```toml\nversion = 1\n```\n\nActual description here.\n",
            "t",
        );
        assert_eq!(value, "Actual description here.");
    }

    #[test]
    fn inline_markup_is_stripped_from_signatures() {
        assert_eq!(
            clean_inline("Read **[`sdlc.md`](workflow/sdlc.md)** first."),
            "Read sdlc.md first."
        );
    }

    #[test]
    fn a_signature_matching_its_ref_is_redundant() {
        assert!(is_redundant("Foo bar", "foo-bar"));
        assert!(is_redundant("Nix patterns", "nix/nix-patterns"));
        assert!(!is_redundant("Foo bar baz", "foo-bar"));
    }

    #[test]
    fn humanize_uses_the_last_path_segment() {
        assert_eq!(humanize("nix/nix_patterns"), "Nix patterns");
    }

    #[test]
    fn long_signatures_truncate_on_a_word_boundary() {
        let long = "word ".repeat(40);
        let rendered = truncate(long.trim());
        assert!(rendered.ends_with('…'));
        assert!(rendered.chars().count() <= 121);
    }

    #[test]
    fn outline_is_computed_from_a_directory_scan() {
        let root = crate::test_support::temp_dir("outline-scan");
        let global_root = root.join("global");
        std::fs::create_dir_all(global_root.join("memory/nix")).unwrap();
        std::fs::write(
            global_root.join("rata.toml"),
            "version = 1\n\n[stores]\nmemory = \"memory\"\n",
        )
        .unwrap();
        std::fs::write(
            global_root.join("memory/containerized-agents.md"),
            "# Containerized agents\n\nScope rules for boxed drivers.\n",
        )
        .unwrap();
        std::fs::write(
            global_root.join("memory/nix/patterns.md"),
            "---\ndescription: Nix patterns worth reusing\n---\n# Patterns\n",
        )
        .unwrap();
        std::fs::write(global_root.join("memory/.hidden.md"), "# Hidden\n").unwrap();

        let report = build_outline(&root, Some(&global_root), Some("memory"), None).unwrap();
        let nodes = &report.stores[0].nodes;
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].reference, "containerized-agents");
        assert_eq!(nodes[0].signature, "Scope rules for boxed drivers.");
        assert_eq!(nodes[1].reference, "nix/patterns");
        assert_eq!(nodes[1].tier, SignatureTier::Description);
        assert_eq!(nodes[1].depth, 2);

        // A new file appears with no other edit, and --depth caps nesting.
        std::fs::write(global_root.join("memory/fresh.md"), "# Fresh\n").unwrap();
        let capped = build_outline(&root, Some(&global_root), Some("memory"), Some(1)).unwrap();
        let refs = capped.stores[0]
            .nodes
            .iter()
            .map(|node| node.reference.as_str())
            .collect::<Vec<_>>();
        assert_eq!(refs, vec!["containerized-agents", "fresh"]);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_heading_that_restates_the_ref_keeps_only_the_informative_half() {
        let (value, tier) = signature(
            "# ask-2026-07-17-rebuild-sync — Rebuild safe two-way sync baselines\n",
            "ask-2026-07-17-rebuild-sync",
        );
        assert_eq!(value, "Rebuild safe two-way sync baselines");
        assert_eq!(tier, SignatureTier::Heading);
    }

    #[test]
    fn a_label_fragment_is_rejected_in_favour_of_the_heading() {
        // `**Original:**` is a lead-in label, not a description; the H1 is strictly better.
        let (value, tier) = signature(
            "# 25 — Add an offset param\n\n**Original:**\n\nSome quoted ask.\n",
            "25",
        );
        assert_eq!(value, "Add an offset param");
        assert_eq!(tier, SignatureTier::Heading);
    }

    #[test]
    fn ordered_lists_are_structure_too() {
        let (value, tier) = signature("# Phases\n\n1. do this\n2. then this\n", "phases");
        assert_eq!(value, "Phases");
        assert_eq!(tier, SignatureTier::Heading);
    }

    #[test]
    fn a_redundant_signature_renders_as_the_ref_alone() {
        let root = crate::test_support::temp_dir("outline-redundant");
        let global_root = root.join("global");
        std::fs::create_dir_all(global_root.join("memory")).unwrap();
        std::fs::write(
            global_root.join("rata.toml"),
            "version = 1\n\n[stores]\nmemory = \"memory\"\n",
        )
        .unwrap();
        std::fs::write(global_root.join("memory/foo-bar.md"), "# Foo bar\n").unwrap();

        let rendered = build_outline(&root, Some(&global_root), Some("memory"), None)
            .unwrap()
            .to_string();
        assert!(rendered.contains("- foo-bar\n"));
        assert!(!rendered.contains("foo-bar — Foo bar"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_shadowed_layer_is_recorded_not_dropped() {
        let root = crate::test_support::temp_dir("outline-shadow");
        let global_root = root.join("global");
        let project = root.join("project");
        std::fs::create_dir_all(global_root.join("memory")).unwrap();
        std::fs::create_dir_all(project.join(".rata/memory")).unwrap();
        std::fs::write(
            global_root.join("rata.toml"),
            "version = 1\n\n[stores]\nmemory = { path = \"memory\", composition = \"global-first\" }\n",
        )
        .unwrap();
        std::fs::write(
            project.join("rata.toml"),
            "version = 1\n\n[stores]\nmemory = { path = \".rata/memory\" }\n",
        )
        .unwrap();
        std::fs::write(global_root.join("memory/note.md"), "# Global note\n").unwrap();
        std::fs::write(project.join(".rata/memory/note.md"), "# Local note\n").unwrap();

        let report = build_outline(&project, Some(&global_root), Some("memory"), None).unwrap();
        let node = &report.stores[0].nodes[0];
        assert_eq!(node.path, global_root.join("memory/note.md"));
        assert_eq!(node.shadowed, vec![project.join(".rata/memory/note.md")]);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unreadable_file_becomes_a_reportable_node_instead_of_aborting_the_scan() {
        let root = crate::test_support::temp_dir("outline-unreadable");
        let global_root = root.join("global");
        std::fs::create_dir_all(global_root.join("memory")).unwrap();
        std::fs::write(
            global_root.join("rata.toml"),
            "version = 1\n\n[stores]\nmemory = \"memory\"\n",
        )
        .unwrap();
        std::fs::write(
            global_root.join("memory/good.md"),
            "# Good\n\nReadable prose.\n",
        )
        .unwrap();
        // Invalid UTF-8 is the realistic case: a stray binary or latin-1 file in a store.
        std::fs::write(global_root.join("memory/bad.md"), [0xff, 0xfe, 0x00]).unwrap();

        let report = build_outline(&root, Some(&global_root), Some("memory"), None).unwrap();
        let nodes = &report.stores[0].nodes;
        assert_eq!(nodes.len(), 2, "the good file is still listed");
        assert!(nodes[0].unreadable.is_some());
        assert!(nodes[1].unreadable.is_none());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_symlinked_directory_is_not_descended_into() {
        let root = crate::test_support::temp_dir("outline-symlink");
        let global_root = root.join("global");
        std::fs::create_dir_all(global_root.join("memory")).unwrap();
        std::fs::write(
            global_root.join("rata.toml"),
            "version = 1\n\n[stores]\nmemory = \"memory\"\n",
        )
        .unwrap();
        std::fs::write(global_root.join("memory/real.md"), "# Real\n").unwrap();
        // A link back to the store would otherwise recurse until the OS refused.
        std::os::unix::fs::symlink(global_root.join("memory"), global_root.join("memory/loop"))
            .unwrap();

        let report = build_outline(&root, Some(&global_root), Some("memory"), None).unwrap();
        let refs = report.stores[0]
            .nodes
            .iter()
            .map(|node| node.reference.as_str())
            .collect::<Vec<_>>();
        assert_eq!(refs, vec!["real"]);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unknown_store_names_the_available_ones() {
        let root = crate::test_support::temp_dir("outline-unknown-store");
        let global_root = root.join("global");
        std::fs::create_dir_all(&global_root).unwrap();
        std::fs::write(
            global_root.join("rata.toml"),
            "version = 1\n\n[stores]\nmemory = \"memory\"\n",
        )
        .unwrap();

        let error = build_outline(&root, Some(&global_root), Some("nope"), None).unwrap_err();
        assert!(error.to_string().contains("memory"));

        std::fs::remove_dir_all(root).unwrap();
    }
}
