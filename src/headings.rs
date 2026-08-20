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
    let mut fence: Option<char> = None;

    for line in setext_to_atx(body) {
        if let Some(marker) = fence_marker(&line) {
            // Track *which* marker opened the fence: a `~~~` inside a ``` block is content, and
            // toggling a single flag on it would end the block early.
            match fence {
                Some(open) if open == marker => fence = None,
                Some(_) => {}
                None => fence = Some(marker),
            }
            push_line(&mut preamble, &mut flat, &line);
            continue;
        }

        // A `#` inside a fence is code or a shell comment, not structure.
        match heading_line(&line).filter(|_| fence.is_none()) {
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
            None => push_line(&mut preamble, &mut flat, &line),
        }
    }

    for heading in &mut flat {
        let (signature, tier) = signature_for(&heading.title, &heading.body);
        heading.signature = signature;
        heading.tier = tier;
    }

    let mut tree = nest(&mut flat.into_iter().peekable(), 0);

    // A lone *H1* is the file's title, not a section within it. Collapsing it means
    // `AGENTS.md#Safety` rather than `AGENTS.md#agents-md-personal-operating-manual/Safety`, and
    // `show AGENTS.md` returns the intro prose plus the section signatures. A file whose only
    // top-level heading is an H2 or deeper is *not* titled by it, so it stays a real section.
    if tree.len() == 1 && tree[0].level == 1 {
        let root = tree.remove(0);
        preamble.push_str(&root.body);
        tree = root.children;
    }

    // Slugs must be unique among siblings and non-empty before addresses are built, or `outline`
    // would print refs that `show` cannot resolve.
    make_slugs_addressable(&mut tree);
    assign_paths(&mut tree, &[]);

    (preamble, tree)
}

/// Rewrite setext headings (a title over `===` or `---`) as their ATX equivalents, so the rest of
/// the parser only has one shape to handle.
fn setext_to_atx(body: &str) -> Vec<String> {
    let lines = body.split_inclusive('\n').collect::<Vec<_>>();
    let mut out = Vec::with_capacity(lines.len());
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        let underlined = lines
            .get(index + 1)
            .and_then(|next| setext_level(next))
            .filter(|_| is_setext_text(line));

        match underlined {
            Some(level) => {
                out.push(format!("{} {}\n", "#".repeat(level), line.trim()));
                index += 2;
            }
            None => {
                out.push(line.to_string());
                index += 1;
            }
        }
    }

    out
}

fn setext_level(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    if trimmed.len() < 2 || leading_indent(line) > 3 {
        return None;
    }
    match trimmed.chars().next()? {
        '=' if trimmed.chars().all(|c| c == '=') => Some(1),
        '-' if trimmed.chars().all(|c| c == '-') => Some(2),
        _ => None,
    }
}

/// Only ordinary prose can carry a setext underline. A blank line, a list item, a quote or an ATX
/// heading followed by dashes is something else — usually a horizontal rule.
fn is_setext_text(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && leading_indent(line) <= 3
        && !trimmed.starts_with('#')
        && !trimmed.starts_with('>')
        && !trimmed.starts_with("- ")
        && !trimmed.starts_with("* ")
        && !trimmed.starts_with("+ ")
        && !trimmed.starts_with('|')
        && fence_marker(line).is_none()
}

fn fence_marker(line: &str) -> Option<char> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("```") {
        Some('`')
    } else if trimmed.starts_with("~~~") {
        Some('~')
    } else {
        None
    }
}

/// Columns of leading whitespace, counting a tab as four. Four or more means an indented code
/// block, where a `#` is content rather than a heading.
fn leading_indent(line: &str) -> usize {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
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
) -> Vec<Heading> {
    let mut out = Vec::new();

    while let Some(next) = flat.peek() {
        if next.level <= parent_level {
            break;
        }
        let mut heading = flat.next().expect("peeked");
        heading.children = nest(flat, heading.level);
        out.push(heading);
    }

    out
}

/// Give every heading a slug that can actually be typed back: non-empty, and unique among its
/// siblings. Two `## Notes` sections become `notes` and `notes-2`.
fn make_slugs_addressable(headings: &mut [Heading]) {
    let mut used = std::collections::BTreeMap::<String, usize>::new();

    for heading in headings.iter_mut() {
        if heading.slug.is_empty() {
            heading.slug = "section".to_string();
        }
        let count = used.entry(heading.slug.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            heading.slug = format!("{}-{count}", heading.slug);
        }
        make_slugs_addressable(&mut heading.children);
    }
}

fn assign_paths(headings: &mut [Heading], parent: &[String]) {
    for heading in headings.iter_mut() {
        let mut path = parent.to_vec();
        path.push(heading.slug.clone());
        assign_paths(&mut heading.children, &path);
        heading.path = path;
    }
}

/// An ATX heading: up to three columns of indent, one to six `#`, then whitespace and text. A
/// trailing run of `#` is a closing sequence, not part of the title.
fn heading_line(line: &str) -> Option<(usize, &str)> {
    if leading_indent(line) > 3 {
        return None;
    }
    let trimmed = line.trim();
    let level = trimmed.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = trimmed[level..].strip_prefix([' ', '\t'])?;
    let text = rest.trim().trim_end_matches('#').trim_end();
    (!text.is_empty()).then_some((level, text))
}

/// `## PR summaries {#pr-sums}` yields the title `PR summaries` and the anchor `pr-sums`.
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
    if !text[close + 1..].trim().is_empty() {
        return (text, None);
    }
    let anchor = text[open + 2..close].trim();
    // `/` and `#` are the ref separators, so an anchor containing one could never be addressed.
    // Drop back to slugifying the title rather than minting an unusable ref.
    if anchor.is_empty() || anchor.contains(['/', '#']) {
        return (text[..open].trim(), None);
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
    fn a_lone_h2_is_a_real_section_and_is_not_collapsed() {
        // Only an H1 titles a file. Collapsing a lone H2 would delete the section outright.
        let (preamble, tree) = parse("Intro.\n\n## Trailing hashes ##\n\nProse.\n");
        assert_eq!(preamble.trim(), "Intro.");
        assert_eq!(tree.len(), 1);
        // A trailing run of `#` is a closing sequence, not part of the title.
        assert_eq!(tree[0].title, "Trailing hashes");
        assert_eq!(tree[0].address(), "trailing-hashes");
    }

    #[test]
    fn an_indented_code_block_is_not_a_heading() {
        let (_, tree) = parse("# B\n\n- item one\n\n      # indented code hash\n\n## Real\n");
        let titles = tree
            .iter()
            .map(|heading| heading.title.as_str())
            .collect::<Vec<_>>();
        assert_eq!(titles, vec!["Real"]);
    }

    #[test]
    fn a_tab_after_the_hashes_still_opens_a_heading() {
        let (_, tree) = parse("Intro.\n\n##\tTabbed\n");
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].title, "Tabbed");
    }

    #[test]
    fn setext_headings_are_recognized() {
        let (preamble, tree) = parse("Title\n=====\n\nIntro prose.\n\nSection\n-------\n\nMore.\n");
        // The setext H1 titles the file, exactly as an ATX H1 would.
        assert_eq!(preamble.trim(), "Intro prose.");
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].title, "Section");
        assert_eq!(tree[0].level, 2);
    }

    #[test]
    fn a_horizontal_rule_is_not_a_setext_underline() {
        let (_, tree) = parse("# One\n\nProse.\n\n---\n\n## Two\n");
        let titles = tree
            .iter()
            .map(|heading| heading.title.as_str())
            .collect::<Vec<_>>();
        assert_eq!(titles, vec!["Two"]);
    }

    #[test]
    fn a_tilde_fence_does_not_close_a_backtick_fence() {
        let (_, tree) = parse("# One\n\n```\n~~~\n# not a heading\n```\n\n## Real\n");
        let titles = tree
            .iter()
            .map(|heading| heading.title.as_str())
            .collect::<Vec<_>>();
        assert_eq!(titles, vec!["Real"]);
    }

    #[test]
    fn every_printed_slug_is_non_empty_and_unique_among_siblings() {
        let (_, tree) = parse("Intro.\n\n## Dup\n\n## Dup\n\n## ---\n\n## ***\n");
        let slugs = tree
            .iter()
            .map(|heading| heading.address())
            .collect::<Vec<_>>();
        assert_eq!(slugs, vec!["dup", "dup-2", "section", "section-2"]);
    }

    #[test]
    fn an_anchor_containing_a_ref_separator_is_refused() {
        // `a/b` would parse as a two-segment heading path and never resolve, so the title wins.
        let (_, tree) = parse("Intro.\n\n## Weird {#a/b}\n");
        assert_eq!(tree[0].anchor, None);
        assert_eq!(tree[0].address(), "weird");
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
