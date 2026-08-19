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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<frontmatter::FrontmatterIssue>,
}

impl Node {
    /// True when this node's frontmatter tries to decide its own eagerness — a hard error.
    pub fn has_eagerness_key(&self) -> bool {
        frontmatter::has_eagerness_key(&self.issues)
    }
}

/// Which rung of the fallback ladder produced a node's signature. Lower is better.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
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

    resolved
        .iter()
        .filter(|(name, _)| store.is_none_or(|wanted| wanted == name.as_str()))
        .map(|(name, resolved_store)| {
            // A scope that is both the global root and a local root contributes the same layer
            // twice; scanning it once is enough.
            let mut paths = resolved_store.paths.clone();
            paths.dedup();
            Ok(OutlineStore {
                nodes: collect_nodes(name, &paths, depth)?,
                name: name.clone(),
                paths,
            })
        })
        .collect()
}

/// Scan every layer of a store, in resolved precedence order, into nodes addressed by ref.
///
/// The index is computed here rather than read from a file, so there is nothing to keep in sync.
/// A ref found in more than one layer resolves to the first layer that has it.
fn collect_nodes(store: &str, paths: &[PathBuf], depth: Option<usize>) -> Result<Vec<Node>> {
    let mut nodes = Vec::new();
    let mut seen = BTreeSet::new();

    for root in paths {
        let mut files = Vec::new();
        collect_markdown_files(root, &mut files)?;
        files.sort();

        for path in files {
            let Some(reference) = node_ref(root, &path) else {
                continue;
            };
            let node_depth = reference.split('/').count();
            if depth.is_some_and(|limit| node_depth > limit) {
                continue;
            }
            if !seen.insert(reference.clone()) {
                continue;
            }
            nodes.push(read_node(store, &reference, &path, node_depth)?);
        }
    }

    nodes.sort_by(|left, right| left.reference.cmp(&right.reference));
    Ok(nodes)
}

fn collect_markdown_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        // A declared store layer that does not exist yet simply contributes nothing.
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(RatatoskrError::ReadStoreDir(root.to_path_buf(), source)),
    };

    for entry in entries {
        let entry =
            entry.map_err(|source| RatatoskrError::ReadStoreDir(root.to_path_buf(), source))?;
        let path = entry.path();
        if is_hidden(&path) {
            continue;
        }
        if path.is_dir() {
            collect_markdown_files(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            files.push(path);
        }
    }

    Ok(())
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

fn node_ref(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let stem = relative.with_extension("");
    let text = stem.to_str()?.replace(std::path::MAIN_SEPARATOR, "/");
    (!text.is_empty()).then_some(text)
}

fn read_node(store: &str, reference: &str, path: &Path, depth: usize) -> Result<Node> {
    let contents = fs::read_to_string(path)
        .map_err(|source| RatatoskrError::ReadContextFile(path.to_path_buf(), source))?;
    let (front, body) = frontmatter::parse(&contents);
    let (signature, tier) = resolve_signature(&front, body, reference);

    Ok(Node {
        reference: reference.to_string(),
        store: store.to_string(),
        path: path.to_path_buf(),
        depth,
        redundant: is_redundant(&signature, reference),
        signature,
        tier,
        tags: front.tags.clone(),
        issues: front.issues,
    })
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
    (!sentence.is_empty()).then(|| sentence.to_string())
}

fn is_structural(line: &str) -> bool {
    line.starts_with('#')
        || line.starts_with('>')
        || line.starts_with('|')
        || line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("<!--")
        || line.starts_with("@")
        || line.chars().all(|c| c == '-' || c == '=')
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
