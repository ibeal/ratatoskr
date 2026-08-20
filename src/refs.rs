//! One ref syntax for everything addressable.
//!
//! ```text
//! memory:containerized-agents          a store node
//! AGENTS.md#Safety                     a heading in a context file
//! workflow/sdlc.md#Phases/PR summaries a nested heading, path-addressed
//! memory:nix#Patterns                  a heading inside a store node
//! ```
//!
//! Files and headings are the same kind of thing at different scales, so one parser and one
//! resolver serve `show`, `outline`, and (later) `callers`.

use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::errors::{RatatoskrError, Result};
use crate::frontmatter::{self, Frontmatter};
use crate::headings::{self, Heading};
use crate::outline::{self, SignatureTier};
use crate::resolve::{self, ResolvedManifest};

/// A ref as written, split into the part that names a file and the part that names a heading inside
/// it. Parsing never fails — an unresolvable ref is a resolution error, where the candidate list
/// can be built.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Ref {
    /// `Some(store)` for `store:node`, `None` for a bare path.
    pub store: Option<String>,
    /// The store-relative node ref, or the file path as written.
    pub target: String,
    /// Heading path segments after `#`, if any.
    pub heading: Vec<String>,
}

impl Ref {
    pub fn parse(text: &str) -> Self {
        let (address, heading) = match text.split_once('#') {
            Some((address, heading)) => (
                address,
                heading
                    .split('/')
                    .map(str::trim)
                    .filter(|segment| !segment.is_empty())
                    .map(str::to_string)
                    .collect(),
            ),
            None => (text, Vec::new()),
        };

        // A colon means a store ref, but only when what precedes it looks like a store name. A
        // Windows-style drive or a URL-ish string stays a path.
        let (store, target) = match address.split_once(':') {
            Some((store, target)) if is_store_name(store) => {
                (Some(store.to_string()), target.to_string())
            }
            _ => (None, address.to_string()),
        };

        Self {
            store,
            target,
            heading,
        }
    }

    /// The file part of the ref, without the heading.
    pub fn file_address(&self) -> String {
        match &self.store {
            Some(store) => format!("{store}:{}", self.target),
            None => self.target.clone(),
        }
    }
}

impl Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.file_address())?;
        if !self.heading.is_empty() {
            write!(f, "#{}", self.heading.join("/"))?;
        }
        Ok(())
    }
}

fn is_store_name(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('/')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A resolved node: a whole file, or one heading inside one.
#[derive(Debug, Serialize)]
pub struct ResolvedRef {
    #[serde(rename = "ref")]
    pub reference: String,
    pub path: PathBuf,
    pub kind: RefKind,
    pub signature: String,
    pub tier: SignatureTier,
    /// The node's own prose, excluding its descendants.
    pub body: String,
    pub children: Vec<ChildRef>,
    /// The heading, when the ref addressed one. Carries the subtree for deeper renders.
    #[serde(skip_serializing)]
    pub heading: Option<Heading>,
    /// Every top-level heading of the file, when the ref addressed the file itself.
    #[serde(skip_serializing)]
    pub file_headings: Vec<Heading>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RefKind {
    File,
    Heading,
}

#[derive(Debug, Serialize)]
pub struct ChildRef {
    #[serde(rename = "ref")]
    pub reference: String,
    pub signature: String,
    pub tier: SignatureTier,
}

/// Everything a ref can name, built once so resolution and candidate suggestions share a view.
pub struct RefSpace {
    /// Addressable file refs → the file on disk. One file may have several addresses.
    files: BTreeMap<String, PathBuf>,
    /// The one address to *print* for a file. Several spellings resolve; only one is canonical.
    canonical: BTreeMap<PathBuf, String>,
}

impl RefSpace {
    pub fn build(manifest: &ResolvedManifest) -> Result<Self> {
        let mut files = BTreeMap::new();
        let mut canonical = BTreeMap::<PathBuf, String>::new();

        // Context files are addressable by their path relative to the scope that declared them,
        // which is how they are written in rata.toml and in prose links.
        for entry in &manifest.context_entries {
            let Some(path) = entry.path() else { continue };
            if let Ok(relative) = path.strip_prefix(&entry.scope_root) {
                let address = slashed(relative);
                files.insert(address.clone(), path.to_path_buf());
                prefer_canonical(&mut canonical, path, address);
            }
            files.insert(path.display().to_string(), path.to_path_buf());
            prefer_canonical(&mut canonical, path, path.display().to_string());
        }

        for store in outline::outline_stores(&manifest.stores, None, None)? {
            for node in store.nodes {
                let address = format!("{}:{}", store.name, node.reference);
                prefer_canonical(&mut canonical, &node.path, address.clone());
                files.insert(address, node.path);
            }
        }

        Ok(Self { files, canonical })
    }

    /// Every addressable file, by canonical ref. This is the graph's node set.
    pub fn nodes(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.canonical
            .iter()
            .map(|(path, address)| (address.as_str(), path.as_path()))
    }

    /// The ref to print for a file, if the file is addressable at all.
    pub fn canonical_ref(&self, path: &Path) -> Option<&str> {
        self.canonical.get(path).map(String::as_str)
    }

    /// Resolve an address as written to its canonical ref, without reading the file.
    pub fn lookup_ref(&self, address: &str) -> Option<&str> {
        let path = self.lookup(address)?;
        self.canonical.get(&path).map(String::as_str)
    }

    /// Resolve a ref, or fail with the closest things that do exist. A bare error here would send
    /// the caller back to guessing, which is the whole problem refs are meant to remove.
    pub fn resolve(&self, reference: &Ref) -> Result<ResolvedRef> {
        let address = reference.file_address();
        let path = self
            .lookup(&address)
            .ok_or_else(|| RatatoskrError::UnresolvedRef {
                reference: reference.to_string(),
                candidates: self.candidates(&address),
            })?;

        let contents = fs::read_to_string(&path)
            .map_err(|source| RatatoskrError::ReadContextFile(path.clone(), source))?;
        let (front, body) = frontmatter::parse(&contents);
        let (preamble, tree) = headings::parse(body);

        if reference.heading.is_empty() {
            return Ok(file_ref(reference, &path, &front, body, preamble, tree));
        }

        let heading = find_heading(&tree, &reference.heading).ok_or_else(|| {
            RatatoskrError::UnresolvedRef {
                reference: reference.to_string(),
                candidates: heading_candidates(&tree, &address),
            }
        })?;

        Ok(ResolvedRef {
            reference: format!("{address}#{}", heading.address()),
            path,
            kind: RefKind::Heading,
            signature: heading.signature.clone(),
            tier: heading.tier,
            body: heading.body.clone(),
            children: heading
                .children
                .iter()
                .map(|child| ChildRef {
                    reference: format!("{address}#{}", child.address()),
                    signature: child.signature.clone(),
                    tier: child.tier,
                })
                .collect(),
            heading: Some(heading.clone()),
            file_headings: Vec::new(),
        })
    }

    /// Exact match first, then a unique suffix match, so `sdlc.md` finds
    /// `workflow/sdlc.md` without anyone typing the full path.
    fn lookup(&self, address: &str) -> Option<PathBuf> {
        if let Some(path) = self.files.get(address) {
            return Some(path.clone());
        }

        // Distinct addresses that name the same file (the relative and absolute forms of one
        // context file) are not an ambiguity.
        let suffix = format!("/{address}");
        let matches = self
            .files
            .iter()
            .filter(|(known, _)| known.ends_with(&suffix))
            .map(|(_, path)| path.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let mut matches = matches.into_iter();
        let first = matches.next()?;
        // A shorthand that really is ambiguous is not a resolution; make the caller disambiguate.
        matches.next().is_none().then_some(first)
    }

    fn candidates(&self, address: &str) -> Vec<String> {
        let needle = address.rsplit(['/', ':']).next().unwrap_or(address);
        let mut scored = self
            .files
            .keys()
            .map(|known| {
                let tail = known.rsplit(['/', ':']).next().unwrap_or(known);
                (similarity(needle, tail), known.clone())
            })
            .collect::<Vec<_>>();
        // Best match first; among equals prefer the scope-relative form, since that is how refs are
        // meant to be written and the absolute path is only its duplicate.
        scored.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.starts_with('/').cmp(&right.1.starts_with('/')))
                .then_with(|| left.1.cmp(&right.1))
        });
        scored
            .into_iter()
            .filter(|(score, _)| *score > 0)
            .take(5)
            .map(|(_, known)| known)
            .collect()
    }
}

fn file_ref(
    reference: &Ref,
    path: &Path,
    front: &Frontmatter,
    body: &str,
    preamble: String,
    tree: Vec<Heading>,
) -> ResolvedRef {
    let address = reference.file_address();
    let name = address.rsplit([':', '/']).next().unwrap_or(&address);
    let (signature, tier) = outline::resolve_signature(front, body, name);

    // A file with no headings has no descendants to exclude, so its own body is the whole file.
    // That is why `show memory:some-note` returns the note.
    let own_body = if tree.is_empty() {
        body.to_string()
    } else {
        preamble
    };

    ResolvedRef {
        reference: address.clone(),
        path: path.to_path_buf(),
        kind: RefKind::File,
        signature,
        tier,
        body: own_body,
        children: tree
            .iter()
            .map(|child| ChildRef {
                reference: format!("{address}#{}", child.address()),
                signature: child.signature.clone(),
                tier: child.tier,
            })
            .collect(),
        heading: None,
        file_headings: tree,
    }
}

/// Walk a heading path. Each segment matches an explicit anchor, the slugified title, or the title
/// itself — so `#PR-summaries` and `#PR summaries` both land on the same heading.
fn find_heading<'a>(tree: &'a [Heading], path: &[String]) -> Option<&'a Heading> {
    let (segment, rest) = path.split_first()?;
    // The H1 is collapsed into the file node by the parser, so a top-level section like `#Safety`
    // matches here directly and needs no skip-the-title special case.
    let matched = tree
        .iter()
        .find(|heading| segment_matches(heading, segment))?;

    if rest.is_empty() {
        return Some(matched);
    }
    find_heading(&matched.children, rest)
}

fn segment_matches(heading: &Heading, segment: &str) -> bool {
    let wanted = headings::slugify(segment);
    heading.slug == segment || heading.slug == wanted || headings::slugify(&heading.title) == wanted
}

fn heading_candidates(tree: &[Heading], address: &str) -> Vec<String> {
    let mut out = Vec::new();
    for heading in tree {
        heading.walk(&mut |node| out.push(format!("{address}#{}", node.address())));
    }
    out.truncate(10);
    out
}

/// Longest common substring length — cheap, and good enough to rank "did you mean" candidates.
fn similarity(needle: &str, candidate: &str) -> usize {
    let needle = needle.to_lowercase();
    let candidate = candidate.to_lowercase();
    let needle: Vec<char> = needle.chars().collect();
    let candidate: Vec<char> = candidate.chars().collect();
    let mut best = 0;
    let mut previous = vec![0usize; candidate.len() + 1];

    for left in &needle {
        let mut current = vec![0usize; candidate.len() + 1];
        for (column, right) in candidate.iter().enumerate() {
            if left == right {
                current[column + 1] = previous[column] + 1;
                best = best.max(current[column + 1]);
            }
        }
        previous = current;
    }

    best
}

/// Prefer a store ref over a relative path over an absolute one, then the shorter of equals — the
/// most specific, least verbose way to name the file.
fn prefer_canonical(canonical: &mut BTreeMap<PathBuf, String>, path: &Path, address: String) {
    let rank = |value: &str| match () {
        _ if value.contains(':') => 0,
        _ if !value.starts_with('/') => 1,
        _ => 2,
    };
    match canonical.get(path) {
        Some(existing) if (rank(existing), existing.len()) <= (rank(&address), address.len()) => {}
        _ => {
            canonical.insert(path.to_path_buf(), address);
        }
    }
}

fn slashed(path: &Path) -> String {
    path.display()
        .to_string()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

/// Build the ref space for a working directory.
pub fn ref_space(
    cwd: &Path,
    global_root_override: Option<&Path>,
    selected_profiles: &[String],
) -> Result<(RefSpace, ResolvedManifest)> {
    let manifest = resolve::resolve_manifest(cwd, global_root_override, selected_profiles)?;
    let space = RefSpace::build(&manifest)?;
    Ok((space, manifest))
}

#[cfg(test)]
mod tests {
    use super::{Ref, similarity};

    #[test]
    fn a_store_ref_is_distinguished_from_a_path() {
        let store = Ref::parse("memory:containerized-agents");
        assert_eq!(store.store.as_deref(), Some("memory"));
        assert_eq!(store.target, "containerized-agents");
        assert!(store.heading.is_empty());

        let file = Ref::parse("workflow/sdlc.md");
        assert!(file.store.is_none());
        assert_eq!(file.target, "workflow/sdlc.md");
    }

    #[test]
    fn a_heading_path_splits_on_slashes() {
        let nested = Ref::parse("workflow/sdlc.md#Phases/PR summaries");
        assert!(nested.store.is_none());
        assert_eq!(nested.target, "workflow/sdlc.md");
        assert_eq!(nested.heading, vec!["Phases", "PR summaries"]);
        assert_eq!(nested.to_string(), "workflow/sdlc.md#Phases/PR summaries");
    }

    #[test]
    fn a_heading_inside_a_store_node_uses_the_same_syntax() {
        let both = Ref::parse("memory:nix#Patterns");
        assert_eq!(both.store.as_deref(), Some("memory"));
        assert_eq!(both.target, "nix");
        assert_eq!(both.heading, vec!["Patterns"]);
        assert_eq!(both.file_address(), "memory:nix");
    }

    #[test]
    fn a_colon_that_is_not_a_store_name_stays_part_of_the_path() {
        let path = Ref::parse("notes/a:b/c.md");
        assert!(path.store.is_none());
        assert_eq!(path.target, "notes/a:b/c.md");
    }

    #[test]
    fn similarity_ranks_a_near_miss_above_an_unrelated_name() {
        assert!(similarity("sdlc", "sdlc.md") > similarity("sdlc", "identity.md"));
    }
}
