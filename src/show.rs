use std::fmt::{self, Display};
use std::path::Path;

use serde::Serialize;

use crate::errors::Result;
use crate::headings::Heading;
use crate::outline;
use crate::refs::{self, ChildRef, Ref, RefKind, ResolvedRef};

#[derive(Debug, Serialize)]
pub struct ShowReport {
    #[serde(flatten)]
    pub node: ResolvedRef,
    pub depth: usize,
    /// Descendant bodies, present only when `depth` asked for them.
    pub descendants: Vec<ShowSection>,
}

#[derive(Debug, Serialize)]
pub struct ShowSection {
    #[serde(rename = "ref")]
    pub reference: String,
    pub level: usize,
    pub title: String,
    pub body: String,
}

/// Read one node.
///
/// `depth` 0 is the default and the useful one: the node's own body plus the **signatures** of its
/// children, so you can see what is below without paying for it. `depth` N descends N levels with
/// bodies included. The rule is the same for files and headings — a file's children are its
/// top-level headings — so there is one model to remember rather than two.
pub fn build_show(
    cwd: &Path,
    global_root_override: Option<&Path>,
    selected_profiles: &[String],
    reference: &str,
    depth: usize,
) -> Result<ShowReport> {
    let (space, _) = refs::ref_space(cwd, global_root_override, selected_profiles)?;
    let node = space.resolve(&Ref::parse(reference))?;

    let subtree: &[Heading] = match &node.heading {
        Some(heading) => &heading.children,
        None => &node.file_headings,
    };
    let mut descendants = Vec::new();
    if depth > 0 {
        collect_sections(&node.reference_base(), subtree, depth, &mut descendants);
    }

    Ok(ShowReport {
        node,
        depth,
        descendants,
    })
}

fn collect_sections(
    address: &str,
    headings: &[Heading],
    remaining: usize,
    out: &mut Vec<ShowSection>,
) {
    if remaining == 0 {
        return;
    }
    for heading in headings {
        out.push(ShowSection {
            reference: format!("{address}#{}", heading.address()),
            level: heading.level,
            title: heading.title.clone(),
            body: heading.body.clone(),
        });
        collect_sections(address, &heading.children, remaining - 1, out);
    }
}

impl ResolvedRef {
    /// The file part of this node's ref, for addressing its descendants.
    fn reference_base(&self) -> String {
        self.reference
            .split_once('#')
            .map(|(base, _)| base.to_string())
            .unwrap_or_else(|| self.reference.clone())
    }
}

impl Display for ShowReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "# Ratatoskr Show")?;
        writeln!(f)?;
        writeln!(f, "ref: {}", self.node.reference)?;
        writeln!(f, "path: {}", self.node.path.display())?;
        writeln!(f, "kind: {}", kind_label(self.node.kind))?;
        writeln!(f, "signature: {}", self.node.signature)?;
        writeln!(f, "tier: {}", self.node.tier.label())?;
        writeln!(f, "depth: {}", self.depth)?;
        writeln!(f)?;

        write_body(f, &self.node.body)?;

        for section in &self.descendants {
            writeln!(f)?;
            writeln!(f, "{} {}", "#".repeat(section.level), section.title)?;
            writeln!(f)?;
            write_body(f, &section.body)?;
        }

        // At depth 0 the children are listed as signatures only: enough to choose the next step,
        // without loading it.
        if self.descendants.is_empty() && !self.node.children.is_empty() {
            writeln!(f)?;
            writeln!(f, "## Children")?;
            writeln!(f)?;
            for child in &self.node.children {
                write_child(f, child)?;
            }
        }

        Ok(())
    }
}

fn write_body(f: &mut fmt::Formatter<'_>, body: &str) -> fmt::Result {
    // A body always starts just after its heading line, so it opens with a blank line that the
    // renderer has already emitted.
    let body = body.trim();
    if body.is_empty() {
        return writeln!(f, "<no prose of its own>");
    }
    writeln!(f, "{body}")
}

fn write_child(f: &mut fmt::Formatter<'_>, child: &ChildRef) -> fmt::Result {
    writeln!(
        f,
        "- {} — {}",
        child.reference,
        outline::truncate(&child.signature)
    )
}

fn kind_label(kind: RefKind) -> &'static str {
    match kind {
        RefKind::File => "file",
        RefKind::Heading => "heading",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::build_show;
    use crate::outline;
    use crate::refs::RefKind;
    use crate::test_support::temp_dir;

    const SDLC: &str = "\
# Build workflow

How to drive a unit of work from should-we to merged.

## Phases

Drive the current phase and update the journal.

### PR summaries {#pr-sums}

A PR body is a pointer, not a report.

## Rules

- one per line
";

    fn fixture(label: &str) -> (PathBuf, PathBuf) {
        let root = temp_dir(label);
        let global_root = root.join("global");
        fs::create_dir_all(global_root.join("workflow")).unwrap();
        fs::create_dir_all(global_root.join("memory")).unwrap();
        fs::write(
            global_root.join("rata.toml"),
            "version = 1\n\n[context]\ninclude = [\"AGENTS.md\", \"workflow/sdlc.md\"]\n\n[stores]\nmemory = \"memory\"\n",
        )
        .unwrap();
        fs::write(
            global_root.join("AGENTS.md"),
            "# Agents\n\nHow agents work here.\n\n## Safety\n\nConfirm before outward-facing acts.\n",
        )
        .unwrap();
        fs::write(global_root.join("workflow/sdlc.md"), SDLC).unwrap();
        fs::write(
            global_root.join("memory/containers.md"),
            "# Containers\n\nHow to box an agent safely.\n",
        )
        .unwrap();
        (root, global_root)
    }

    #[test]
    fn a_nested_heading_ref_returns_that_section_alone() {
        let (root, global_root) = fixture("show-nested-heading");

        // Slug spelling and heading-text spelling both resolve to the same node.
        for reference in [
            "workflow/sdlc.md#Phases/PR-sums",
            "workflow/sdlc.md#Phases/pr-sums",
            "sdlc.md#Phases/PR-sums",
        ] {
            let report = build_show(&root, Some(&global_root), &[], reference, 0).unwrap();
            assert_eq!(report.node.kind, RefKind::Heading);
            assert_eq!(
                report.node.body.trim(),
                "A PR body is a pointer, not a report."
            );
            assert!(report.node.children.is_empty());
            assert!(report.descendants.is_empty());
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn depth_zero_lists_children_as_signatures_and_depth_one_includes_their_bodies() {
        let (root, global_root) = fixture("show-depth");

        let shallow =
            build_show(&root, Some(&global_root), &[], "workflow/sdlc.md#Phases", 0).unwrap();
        assert_eq!(
            shallow.node.body.trim(),
            "Drive the current phase and update the journal."
        );
        assert_eq!(shallow.node.children.len(), 1);
        assert_eq!(
            shallow.node.children[0].reference,
            "workflow/sdlc.md#phases/pr-sums"
        );
        // The child is listed, not loaded: its signature appears under Children, and no section
        // body is rendered for it.
        assert!(shallow.descendants.is_empty());
        let rendered = shallow.to_string();
        assert!(rendered.contains("## Children"));
        assert!(!rendered.contains("### PR summaries"));

        let deep =
            build_show(&root, Some(&global_root), &[], "workflow/sdlc.md#Phases", 1).unwrap();
        assert_eq!(deep.descendants.len(), 1);
        assert!(deep.to_string().contains("A PR body is a pointer"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_store_node_ref_returns_the_file_body() {
        let (root, global_root) = fixture("show-store-node");

        let report = build_show(&root, Some(&global_root), &[], "memory:containers", 0).unwrap();
        assert_eq!(report.node.kind, RefKind::File);
        // No subheadings, so the file's own body is the whole file.
        assert!(report.node.body.contains("How to box an agent safely."));
        assert!(report.node.children.is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_top_level_section_is_addressed_without_the_h1() {
        let (root, global_root) = fixture("show-h1-collapse");

        let report = build_show(&root, Some(&global_root), &[], "AGENTS.md#Safety", 0).unwrap();
        assert_eq!(report.node.reference, "AGENTS.md#safety");
        assert_eq!(
            report.node.body.trim(),
            "Confirm before outward-facing acts."
        );

        // And the file node carries the H1's prose plus the section signatures.
        let file = build_show(&root, Some(&global_root), &[], "AGENTS.md", 0).unwrap();
        assert_eq!(file.node.body.trim(), "How agents work here.");
        assert_eq!(file.node.children.len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unresolvable_ref_names_the_closest_candidates() {
        let (root, global_root) = fixture("show-unresolvable");

        let error = build_show(&root, Some(&global_root), &[], "workflow/sdlk.md", 0)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unresolved ref"));
        assert!(error.contains("workflow/sdlc.md"), "{error}");

        // A bad heading lists the headings that do exist.
        let heading = build_show(&root, Some(&global_root), &[], "workflow/sdlc.md#Phasez", 0)
            .unwrap_err()
            .to_string();
        assert!(heading.contains("#phases"), "{heading}");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn outline_of_a_file_ref_renders_its_heading_tree() {
        let (root, global_root) = fixture("outline-file-ref");

        let report =
            outline::build_file_outline(&root, Some(&global_root), "workflow/sdlc.md", None)
                .unwrap();
        let file = report.file.as_ref().unwrap();
        let refs = file
            .headings
            .iter()
            .map(|heading| heading.reference.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            refs,
            vec![
                "workflow/sdlc.md#phases",
                "workflow/sdlc.md#phases/pr-sums",
                "workflow/sdlc.md#rules",
            ]
        );
        assert!(report.stores.is_empty());

        // --depth caps the tree the same way it caps a store outline.
        let capped =
            outline::build_file_outline(&root, Some(&global_root), "workflow/sdlc.md", Some(1))
                .unwrap();
        assert_eq!(capped.file.as_ref().unwrap().headings.len(), 2);

        fs::remove_dir_all(root).unwrap();
    }
}
