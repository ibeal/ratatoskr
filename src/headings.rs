//! Intra-file structure. A heading is a node with the same signature/body model as a file: its
//! **body** is the prose under it minus its descendants, and its signature resolves through the
//! same ladder. That is what makes inter-file and intra-file navigation one model at two scales.

use serde::Serialize;

use crate::outline::{self, SignatureTier};

#[derive(Clone, Debug, Serialize)]
pub struct Heading {
    /// `#` count. 1 for an H1.
    pub level: usize,
    /// The heading text, with any `{#anchor}` and inline markup removed.
    pub title: String,
    /// An explicit `{#slug}` anchor, when the author wrote one.
    pub anchor: Option<String>,
    /// How this heading is addressed: the explicit anchor if present, else the slugified title.
    pub slug: String,
    /// Slugs from the outermost heading down to this one — the `#a/b/c` address.
    pub path: Vec<String>,
    /// Prose under this heading, excluding every descendant heading and its prose.
    pub body: String,
    pub signature: String,
    pub tier: SignatureTier,
    pub children: Vec<Heading>,
}

impl Heading {
    /// The `#`-suffix that addresses this heading, e.g. `Phases/PR-summaries`.
    pub fn address(&self) -> String {
        self.path.join("/")
    }

    /// Depth-first walk, self first.
    pub fn walk<'a>(&'a self, visit: &mut impl FnMut(&'a Heading)) {
        visit(self);
        for child in &self.children {
            child.walk(visit);
        }
    }
}

/// Parse a file body into its heading forest. Anything before the first heading belongs to the file
/// node itself, not to any heading, and is returned separately.
pub fn parse(body: &str) -> (String, Vec<Heading>) {
    let mut preamble = String::new();
    let mut flat: Vec<Heading> = Vec::new();
    let mut fenced = false;

    for line in body.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            push_line(&mut preamble, &mut flat, line);
            continue;
        }

        // A `#` inside a fence is code or a shell comment, not structure.
        match heading_line(trimmed).filter(|_| !fenced) {
            Some((level, text)) => {
                let (title, anchor) = split_anchor(text);
                let title = outline::clean_inline(title);
                let slug = anchor.clone().unwrap_or_else(|| slugify(&title));
                flat.push(Heading {
                    level,
                    title,
                    anchor,
                    slug,
                    path: Vec::new(),
                    body: String::new(),
                    signature: String::new(),
                    tier: SignatureTier::Heading,
                    children: Vec::new(),
                });
            }
            None => push_line(&mut preamble, &mut flat, line),
        }
    }

    for heading in &mut flat {
        let (signature, tier) = signature_for(&heading.title, &heading.body);
        heading.signature = signature;
        heading.tier = tier;
    }

    let mut tree = nest(&mut flat.into_iter().peekable(), 0, &[]);

    // A lone top-level heading is the file's title, not a section within it. Collapsing it means
    // `AGENTS.md#Safety` rather than `AGENTS.md#agents-md-personal-operating-manual/Safety`, and
    // `show AGENTS.md` returns the intro prose plus the section signatures.
    if tree.len() == 1 && tree[0].children.iter().all(|child| child.level > 1) {
        let root = tree.remove(0);
        preamble.push_str(&root.body);
        tree = root.children;
        drop_root_segment(&mut tree);
    }

    (preamble, tree)
}

fn drop_root_segment(headings: &mut [Heading]) {
    for heading in headings {
        if !heading.path.is_empty() {
            heading.path.remove(0);
        }
        drop_root_segment(&mut heading.children);
    }
}

fn push_line(preamble: &mut String, flat: &mut [Heading], line: &str) {
    match flat.last_mut() {
        Some(heading) => heading.body.push_str(line),
        None => preamble.push_str(line),
    }
}

/// Turn the flat, document-order list into a tree by heading level. Levels may skip (an H1 followed
/// by an H3), so nesting is driven by "deeper than my parent", not by an exact level step.
fn nest(
    flat: &mut std::iter::Peekable<std::vec::IntoIter<Heading>>,
    parent_level: usize,
    parent_path: &[String],
) -> Vec<Heading> {
    let mut out = Vec::new();

    while let Some(next) = flat.peek() {
        if next.level <= parent_level {
            break;
        }
        let mut heading = flat.next().expect("peeked");
        let mut path = parent_path.to_vec();
        path.push(heading.slug.clone());
        heading.children = nest(flat, heading.level, &path);
        heading.path = path;
        out.push(heading);
    }

    out
}

fn heading_line(trimmed: &str) -> Option<(usize, &str)> {
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = trimmed[level..].strip_prefix(' ')?;
    let text = rest.trim();
    (!text.is_empty()).then_some((level, text))
}

/// `## PR summaries {#pr-sums}` → `("PR summaries", Some("pr-sums"))`.
///
/// An explicit anchor is honoured in preference to the heading text, so a cross-referenced section
/// keeps its address when someone rewords the heading.
fn split_anchor(text: &str) -> (&str, Option<String>) {
    let Some(open) = text.rfind("{#") else {
        return (text, None);
    };
    let Some(close) = text[open..].find('}').map(|index| open + index) else {
        return (text, None);
    };
    if text[close + 1..].trim() != "" {
        return (text, None);
    }
    let anchor = text[open + 2..close].trim();
    if anchor.is_empty() {
        return (text, None);
    }
    (text[..open].trim(), Some(anchor.to_string()))
}

/// The address form of a heading title: lowercase, non-alphanumerics collapsed to single dashes.
/// `## PR summaries` and `#PR-summaries` both slugify to `pr-summaries`, so either spelling of a
/// ref resolves.
pub fn slugify(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// The ladder, minus the two rungs that cannot apply: a heading has no frontmatter and no filename.
fn signature_for(title: &str, body: &str) -> (String, SignatureTier) {
    match outline::first_sentence(body, false) {
        Some(sentence) => (sentence, SignatureTier::FirstSentence),
        None => (title.to_string(), SignatureTier::Heading),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, slugify, split_anchor};

    const DOC: &str = "\
# Workflow

Intro prose. More intro.

## Phases

How the phases work.

### PR summaries {#pr-sums}

Ceiling: ~10 lines. A PR body is a pointer.

## Rules

- one per line
";

    #[test]
    fn a_lone_h1_collapses_into_the_file_and_bodies_exclude_descendants() {
        let (preamble, tree) = parse(DOC);
        // The H1 is the file's title, so its prose becomes the file's own body.
        assert_eq!(preamble.trim(), "Intro prose. More intro.");
        assert_eq!(tree.len(), 2);

        let phases = &tree[0];
        assert_eq!(phases.title, "Phases");
        assert_eq!(phases.body.trim(), "How the phases work.");
        assert_eq!(phases.address(), "phases");
        assert_eq!(phases.children.len(), 1);

        let summaries = &phases.children[0];
        assert_eq!(summaries.title, "PR summaries");
        assert_eq!(summaries.anchor.as_deref(), Some("pr-sums"));
        // The explicit anchor wins, and the H1 is not part of the address.
        assert_eq!(summaries.address(), "phases/pr-sums");
    }

    #[test]
    fn a_heading_signature_is_its_first_sentence_then_its_title() {
        let (_, tree) = parse(DOC);
        assert_eq!(tree[0].signature, "How the phases work.");
        // A section with only a list has no prose to draw on.
        assert_eq!(tree[1].signature, "Rules");
    }

    #[test]
    fn hashes_inside_a_fence_are_not_headings() {
        let (_, tree) = parse("# Real\n\n```sh\n# not a heading\n```\n\n## Also real\n");
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].title, "Also real");
    }

    #[test]
    fn a_skipped_level_still_nests_under_its_nearest_ancestor() {
        let (_, tree) = parse("# One\n\n### Three\n\n## Two\n");
        let titles = tree
            .iter()
            .map(|child| child.title.as_str())
            .collect::<Vec<_>>();
        assert_eq!(titles, vec!["Three", "Two"]);
    }

    #[test]
    fn sibling_h1s_are_real_sections_and_are_not_collapsed() {
        let (preamble, tree) = parse("Intro.\n\n# One\n\nA.\n\n# Two\n\nB.\n");
        assert_eq!(preamble.trim(), "Intro.");
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].address(), "one");
        assert_eq!(tree[1].address(), "two");
    }

    #[test]
    fn slugs_and_anchors_normalize_the_same_way() {
        assert_eq!(slugify("PR summaries"), "pr-summaries");
        assert_eq!(slugify("#PR-summaries"), "pr-summaries");
        assert_eq!(slugify("Build & review work"), "build-review-work");
        assert_eq!(
            split_anchor("Safety {#safe}"),
            ("Safety", Some("safe".into()))
        );
        assert_eq!(split_anchor("Not {#an} anchor").1, None);
    }
}
