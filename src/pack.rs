use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cli::{OnlyTarget, ScopeFilter};
use crate::config::EffectiveSettings;
use crate::errors::{RatatoskrError, Result};
use crate::outline;
use crate::resolve::{
    self, ContextSource, ContextTarget, ResolvedContextEntry, ResolvedManifest, ResolvedStore,
};

#[derive(Debug, Serialize)]
pub struct ContextBundle {
    pub cwd: PathBuf,
    pub global_root: Option<PathBuf>,
    pub local_root: Option<PathBuf>,
    pub local_roots: Vec<PathBuf>,
    pub settings: EffectiveSettings,
    pub selected_profiles: Vec<String>,
    pub selector: BundleSelector,
    pub files: Vec<ContextFileEntry>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum BundleSelector {
    Full,
    Profile { name: String },
    Scope { scope: String },
    File { name: String },
}

#[derive(Debug, Serialize)]
pub struct ContextFileEntry {
    #[serde(flatten)]
    pub target: ContextTarget,
    pub scope_kind: String,
    pub scope_root: PathBuf,
    pub source: ContextSource,
    /// True when rata synthesized the body instead of reading it from a file. There is no source
    /// file to open, and editing the output changes nothing.
    pub generated: bool,
    pub contents: String,
}

pub fn build_bundle(
    cwd: &Path,
    global_root_override: Option<&Path>,
    selected_profiles: &[String],
) -> Result<ContextBundle> {
    let manifest = resolve::resolve_manifest(cwd, global_root_override, selected_profiles)?;
    bundle_from_manifest(
        manifest,
        BundleSelector::Full,
        |_: &ResolvedContextEntry| true,
    )
}

pub fn build_only_bundle(
    cwd: &Path,
    global_root_override: Option<&Path>,
    target: &OnlyTarget,
) -> Result<ContextBundle> {
    let selected_profiles = match target {
        OnlyTarget::Profile { name } => vec![name.clone()],
        OnlyTarget::Scope { .. } | OnlyTarget::File { .. } => Vec::new(),
    };
    let manifest = resolve::resolve_manifest(cwd, global_root_override, &selected_profiles)?;

    match target {
        OnlyTarget::Profile { name } => bundle_from_manifest(
            manifest,
            BundleSelector::Profile { name: name.clone() },
            |entry| matches!(&entry.source, ContextSource::Profile { name: profile } if profile == name),
        ),
        OnlyTarget::Scope { kind } => {
            let label = match kind {
                ScopeFilter::Global => "global",
                ScopeFilter::Local => "local",
            };
            bundle_from_manifest(
                manifest,
                BundleSelector::Scope {
                    scope: label.to_string(),
                },
                |entry| entry.scope_kind == label,
            )
        }
        OnlyTarget::File { name } => bundle_from_manifest(
            manifest,
            BundleSelector::File { name: name.clone() },
            |entry| {
                entry
                    .path()
                    .and_then(Path::file_name)
                    .and_then(|value| value.to_str())
                    == Some(name.as_str())
            },
        ),
    }
}

fn bundle_from_manifest(
    manifest: ResolvedManifest,
    selector: BundleSelector,
    predicate: impl Fn(&ResolvedContextEntry) -> bool,
) -> Result<ContextBundle> {
    let mut files = Vec::new();
    let allow_missing = manifest.settings.allow_missing;

    for entry in &manifest.context_entries {
        if !predicate(entry) {
            continue;
        }

        let (contents, generated) = match &entry.target {
            ContextTarget::File { path } => {
                let Some(contents) = read_context_contents(path, allow_missing)? else {
                    continue;
                };
                (contents, false)
            }
            ContextTarget::StoreIndex { store } => {
                let Some(contents) = render_store_index(&manifest.stores, store)? else {
                    continue;
                };
                (contents, true)
            }
        };

        files.push(ContextFileEntry {
            target: entry.target.clone(),
            scope_kind: entry.scope_kind.clone(),
            scope_root: entry.scope_root.clone(),
            source: entry.source.clone(),
            generated,
            contents,
        });
    }

    Ok(ContextBundle {
        cwd: manifest.cwd,
        global_root: manifest.global_root,
        local_root: manifest.local_root,
        local_roots: manifest.local_roots,
        settings: manifest.settings,
        selected_profiles: manifest.selected_profiles,
        selector,
        files,
    })
}

impl Display for ContextBundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "# Ratatoskr Context Pack")?;
        writeln!(f)?;
        writeln!(f, "cwd: {}", self.cwd.display())?;
        writeln!(
            f,
            "global_root: {}",
            display_path(self.global_root.as_ref())
        )?;
        writeln!(f, "local_root: {}", display_path(self.local_root.as_ref()))?;
        writeln!(f, "allow_missing: {}", self.settings.allow_missing)?;
        writeln!(f, "selector: {}", selector_label(&self.selector))?;
        writeln!(f, "selected_profiles:")?;
        if self.selected_profiles.is_empty() {
            writeln!(f, "- <none>")?;
        } else {
            for profile in &self.selected_profiles {
                writeln!(f, "- {profile}")?;
            }
        }
        writeln!(f)?;
        writeln!(f, "## Source Order")?;
        for (index, file) in self.files.iter().enumerate() {
            writeln!(f, "{}. {}", index + 1, file.target.label())?;
        }

        for file in &self.files {
            writeln!(f)?;
            match &file.target {
                ContextTarget::File { path } => writeln!(f, "## File: {}", path.display())?,
                ContextTarget::StoreIndex { store } => writeln!(f, "## Store Index: {store}:")?,
            }
            writeln!(f)?;
            writeln!(f, "scope_kind: {}", file.scope_kind)?;
            writeln!(f, "scope_root: {}", file.scope_root.display())?;
            writeln!(f, "source: {}", source_label(&file.source))?;
            if file.generated {
                // Say so plainly: there is no file to open, and editing this changes nothing.
                writeln!(
                    f,
                    "generated: computed by rata from a directory scan; no source file exists"
                )?;
            }
            writeln!(f)?;
            write!(f, "{}", file.contents)?;
            if !file.contents.ends_with('\n') {
                writeln!(f)?;
            }
        }

        Ok(())
    }
}

/// Render a store's outline as the body of a context entry.
///
/// This is the whole point of a store ref: the index is derived from a directory scan every run, so
/// there is no file to keep in sync and none to go stale. Output is ordered by ref, so two runs over
/// an unchanged store are byte-identical.
fn render_store_index(
    stores: &BTreeMap<String, ResolvedStore>,
    store: &str,
) -> Result<Option<String>> {
    let Some(outline) = outline::outline_stores(stores, Some(store), None)?.pop() else {
        return Ok(None);
    };

    let mut out = String::new();
    out.push_str(&format!("# {store}\n\n"));
    if outline.nodes.is_empty() {
        out.push_str("This store is empty.\n");
        return Ok(Some(out));
    }
    out.push_str(&format!(
        "{} nodes. Read one with `rata only file <name>.md`.\n\n",
        outline.nodes.len()
    ));
    for node in &outline.nodes {
        if node.redundant {
            out.push_str(&format!("- `{store}:{}`\n", node.reference));
        } else {
            out.push_str(&format!(
                "- `{store}:{}` — {}\n",
                node.reference, node.signature
            ));
        }
    }
    Ok(Some(out))
}

fn read_context_contents(path: &Path, allow_missing: bool) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(source) if allow_missing && source.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(RatatoskrError::ReadContextFile(path.to_path_buf(), source)),
    }
}

fn display_path(path: Option<&PathBuf>) -> String {
    match path {
        Some(path) => path.display().to_string(),
        None => "<none>".to_string(),
    }
}

fn selector_label(selector: &BundleSelector) -> String {
    match selector {
        BundleSelector::Full => "full".to_string(),
        BundleSelector::Profile { name } => format!("profile:{name}"),
        BundleSelector::Scope { scope } => format!("scope:{scope}"),
        BundleSelector::File { name } => format!("file:{name}"),
    }
}

fn source_label(source: &ContextSource) -> String {
    match source {
        ContextSource::Base => "base".to_string(),
        ContextSource::Profile { name } => format!("profile:{name}"),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::build_bundle;
    use crate::test_support::temp_dir;

    #[test]
    fn a_store_ref_in_context_include_packs_a_synthesized_index() {
        let root = temp_dir("pack-store-index");
        let global_root = root.join("global");
        fs::create_dir_all(global_root.join("memory")).unwrap();
        fs::write(
            global_root.join("rata.toml"),
            r#"
version = 1

[context]
include = ["AGENTS.md", "memory:"]

[stores]
memory = "memory"
"#,
        )
        .unwrap();
        fs::write(global_root.join("AGENTS.md"), "# Agents\n").unwrap();
        fs::write(
            global_root.join("memory/nix.md"),
            "---\ndescription: Nix patterns worth reusing\n---\n# Nix\n",
        )
        .unwrap();
        fs::write(
            global_root.join("memory/containers.md"),
            "# Containers\n\nHow to box an agent safely.\n",
        )
        .unwrap();

        let bundle = build_bundle(&root, Some(&global_root), &[]).unwrap();

        // The index sits where the include listed it, and is marked as having no source file.
        assert_eq!(bundle.files.len(), 2);
        assert!(!bundle.files[0].generated);
        assert!(bundle.files[1].generated);

        let rendered = bundle.to_string();
        assert!(rendered.contains("## Store Index: memory:"));
        assert!(rendered.contains("generated: computed by rata"));
        assert!(rendered.contains("`memory:containers` — How to box an agent safely."));
        assert!(rendered.contains("`memory:nix` — Nix patterns worth reusing"));

        // Deterministic: an unchanged store packs byte-identically.
        let again = build_bundle(&root, Some(&global_root), &[])
            .unwrap()
            .to_string();
        assert_eq!(rendered, again);

        // A new memory appears with no other edit.
        fs::write(global_root.join("memory/fresh.md"), "# Fresh\n").unwrap();
        let grown = build_bundle(&root, Some(&global_root), &[])
            .unwrap()
            .to_string();
        assert!(grown.contains("`memory:fresh`"));
        assert_ne!(rendered, grown);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unknown_store_ref_is_treated_like_a_missing_file() {
        let root = temp_dir("pack-unknown-store-ref");
        let global_root = root.join("global");
        fs::create_dir_all(&global_root).unwrap();
        fs::write(
            global_root.join("rata.toml"),
            "version = 1\n\n[context]\ninclude = [\"nope:\"]\n",
        )
        .unwrap();

        let bundle = build_bundle(&root, Some(&global_root), &[]).unwrap();
        assert!(bundle.files.is_empty());

        let report = crate::doctor::run_doctor(&root, Some(&global_root), &[]).unwrap();
        assert!(!report.healthy);

        fs::remove_dir_all(root).unwrap();
    }
}
