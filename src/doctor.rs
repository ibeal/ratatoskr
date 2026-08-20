use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::{EffectiveSettings, RemoteStatusKind, SettingsLayer, StoreComposition};
use crate::errors::Result;
use crate::frontmatter::{self, FrontmatterIssue};
use crate::outline::{self, Node, SignatureTier};
use crate::resolve::{
    self, ContextSource, MissingContextFile, ResolvedManifest, ResolvedStoreLayer,
};

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub healthy: bool,
    pub layers: Vec<DoctorLayer>,
    pub errors: Vec<DoctorError>,
    pub warnings: Vec<DoctorWarning>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DoctorWarning {
    /// A store file that looks like a hand-maintained list of its siblings. `pack` can synthesize
    /// that index from a store ref, so the file is a second source of truth that will drift.
    HandMaintainedIndex {
        path: PathBuf,
        store: String,
        /// The sibling refs the file links to.
        links: Vec<String>,
    },
}

#[derive(Debug, Serialize)]
pub struct DoctorLayer {
    pub kind: String,
    pub root: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DoctorError {
    RemoteFile {
        name: String,
        scope_kind: String,
        scope_root: PathBuf,
        destination: PathBuf,
        status: RemoteStatusKind,
        detail: Option<String>,
    },
    MissingContextFile {
        path: PathBuf,
        scope_kind: String,
        scope_root: PathBuf,
        source: ContextSource,
    },
    /// A file trying to decide its own eagerness. `rata.toml` owns topology and eagerness;
    /// frontmatter owns self-description. This is an error, not a warning, because the symptom
    /// (context bloat) shows up far from the cause.
    FrontmatterEagerness { path: PathBuf, key: String },
    /// A `[context].include` store ref naming a store no scope declares.
    UnknownContextStore {
        store: String,
        scope_kind: String,
        scope_root: PathBuf,
    },
}

#[derive(Debug, Serialize)]
pub struct DoctorNodesReport {
    pub healthy: bool,
    pub tier_counts: Vec<DoctorTierCount>,
    pub stores: Vec<DoctorNodeStore>,
}

#[derive(Debug, Serialize)]
pub struct DoctorTierCount {
    pub tier: SignatureTier,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct DoctorNodeStore {
    pub name: String,
    pub nodes: Vec<Node>,
    pub scan_issues: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DoctorStoresReport {
    pub stores: Vec<DoctorStore>,
}

#[derive(Debug, Serialize)]
pub struct DoctorStore {
    pub name: String,
    pub composition: StoreComposition,
    pub layers: Vec<DoctorStoreLayer>,
}

#[derive(Debug, Serialize)]
pub struct DoctorStoreLayer {
    pub scope_kind: String,
    pub scope_root: PathBuf,
    pub path: PathBuf,
    pub composition: Option<StoreComposition>,
}

#[derive(Debug, Serialize)]
pub struct DoctorSettingsReport {
    pub effective: DoctorEffectiveSettings,
    pub layers: Vec<SettingsLayer>,
}

#[derive(Debug, Serialize)]
pub struct DoctorEffectiveSettings {
    pub allow_missing: bool,
    pub global_root: PathBuf,
}

pub fn run_doctor(
    cwd: &Path,
    global_root_override: Option<&Path>,
    selected_profiles: &[String],
) -> Result<DoctorReport> {
    let inspection = resolve::inspect_manifest(cwd, global_root_override, selected_profiles)?;
    let mut errors = doctor_errors(
        &inspection.manifest.remote_files,
        &inspection.missing_context_files,
    );
    errors.extend(eagerness_violations(&inspection.manifest)?);
    errors.extend(inspection.unknown_context_stores.iter().map(|unknown| {
        DoctorError::UnknownContextStore {
            store: unknown.store.clone(),
            scope_kind: unknown.scope_kind.clone(),
            scope_root: unknown.scope_root.clone(),
        }
    }));

    Ok(DoctorReport {
        healthy: errors.is_empty(),
        layers: inspection
            .manifest
            .scopes
            .iter()
            .map(|scope| DoctorLayer {
                kind: scope.kind.clone(),
                root: scope.root.clone(),
            })
            .collect(),
        errors,
        warnings: hand_maintained_indexes(&inspection.manifest)?,
    })
}

/// Flag store files that duplicate what a store ref would synthesize. A file listing most of its
/// siblings is an index someone has to remember to update; `pack` can compute it instead.
fn hand_maintained_indexes(manifest: &ResolvedManifest) -> Result<Vec<DoctorWarning>> {
    let mut warnings = Vec::new();

    for store in outline::outline_stores(&manifest.stores, None, None)? {
        let siblings = store
            .nodes
            .iter()
            .map(|node| node.reference.as_str())
            .collect::<Vec<_>>();

        if siblings.len() < 2 {
            continue;
        }

        for node in &store.nodes {
            let Ok(contents) = fs::read_to_string(&node.path) else {
                continue;
            };
            let others = siblings
                .iter()
                .filter(|sibling| **sibling != node.reference)
                .collect::<Vec<_>>();
            let links = others
                .iter()
                .filter(|sibling| links_to_sibling(&contents, sibling))
                .map(|sibling| (**sibling).to_string())
                .collect::<Vec<_>>();

            if is_pointer_list(&contents, &others) && links.len() * 2 >= others.len() {
                warnings.push(DoctorWarning::HandMaintainedIndex {
                    path: node.path.clone(),
                    store: store.name.clone(),
                    links,
                });
            }
        }
    }

    Ok(warnings)
}

/// The shape that distinguishes an index from prose that happens to cite a sibling: the file *is* a
/// list of pointers. One inline cross-reference in a paragraph is not an index.
fn is_pointer_list(contents: &str, siblings: &[&&str]) -> bool {
    let items = contents
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("- ") || line.starts_with("* "))
        .collect::<Vec<_>>();
    let pointers = items
        .iter()
        .filter(|item| {
            siblings
                .iter()
                .any(|sibling| links_to_sibling(item, sibling))
        })
        .count();

    items.len() >= 2 && pointers * 2 >= items.len() && pointers > 0
}

/// A markdown or `@`-import link whose target resolves to the sibling's file.
fn links_to_sibling(contents: &str, sibling: &str) -> bool {
    let file = format!("{sibling}.md");
    contents.contains(&format!("({file})"))
        || contents.contains(&format!("`{file}`"))
        || contents.contains(&format!("@{file}"))
        || contents.contains(&format!("/{file}"))
}

/// Report where every node's signature came from, so thin signatures are visible without
/// `description:` ever being mandatory.
pub fn run_nodes_doctor(
    cwd: &Path,
    global_root_override: Option<&Path>,
    store: Option<&str>,
) -> Result<DoctorNodesReport> {
    let outline = outline::build_outline(cwd, global_root_override, store, None)?;
    let mut tier_counts = BTreeMap::<&'static str, (SignatureTier, usize)>::new();
    let mut healthy = true;

    for store in &outline.stores {
        // A file rata cannot read is a real problem, not a cosmetic one.
        if !store.scan_issues.is_empty() {
            healthy = false;
        }
        for node in &store.nodes {
            let entry = tier_counts
                .entry(node.tier.label())
                .or_insert((node.tier, 0));
            entry.1 += 1;
            if node.has_eagerness_key() || node.unreadable.is_some() {
                healthy = false;
            }
        }
    }

    Ok(DoctorNodesReport {
        healthy,
        tier_counts: tier_counts
            .into_values()
            .map(|(tier, count)| DoctorTierCount { tier, count })
            .collect(),
        stores: outline
            .stores
            .into_iter()
            .map(|store| DoctorNodeStore {
                name: store.name,
                nodes: store.nodes,
                scan_issues: store.scan_issues,
            })
            .collect(),
    })
}

pub fn run_stores_doctor(
    cwd: &Path,
    global_root_override: Option<&Path>,
    selected_profiles: &[String],
) -> Result<DoctorStoresReport> {
    let manifest = resolve::resolve_manifest(cwd, global_root_override, selected_profiles)?;
    let mut stores = BTreeMap::<String, Vec<DoctorStoreLayer>>::new();

    for scope in manifest.scopes {
        for (name, store) in scope.stores {
            stores.entry(name).or_default().push(DoctorStoreLayer {
                scope_kind: scope.kind.clone(),
                scope_root: scope.root.clone(),
                path: store.path,
                composition: store.composition,
            });
        }
    }

    Ok(DoctorStoresReport {
        stores: stores
            .into_iter()
            .map(|(name, layers)| DoctorStore {
                name,
                composition: effective_composition(&layers),
                layers,
            })
            .collect(),
    })
}

pub fn run_settings_doctor(
    cwd: &Path,
    global_root_override: Option<&Path>,
    selected_profiles: &[String],
) -> Result<DoctorSettingsReport> {
    let manifest = resolve::resolve_manifest(cwd, global_root_override, selected_profiles)?;
    Ok(settings_report(manifest.settings))
}

fn doctor_errors(
    remote_files: &[crate::config::RemoteFileStatus],
    missing_context_files: &[MissingContextFile],
) -> Vec<DoctorError> {
    let mut errors = remote_files
        .iter()
        .filter(|remote| {
            matches!(
                remote.status,
                RemoteStatusKind::Missing | RemoteStatusKind::FetchFailed
            )
        })
        .map(|remote| DoctorError::RemoteFile {
            name: remote.name.clone(),
            scope_kind: remote.scope_kind.clone(),
            scope_root: remote.scope_root.clone(),
            destination: remote.destination.clone(),
            status: remote.status.clone(),
            detail: remote.detail.clone(),
        })
        .collect::<Vec<_>>();
    errors.extend(
        missing_context_files
            .iter()
            .map(|missing| DoctorError::MissingContextFile {
                path: missing.path.clone(),
                scope_kind: missing.scope_kind.clone(),
                scope_root: missing.scope_root.clone(),
                source: missing.source.clone(),
            }),
    );
    errors
}

/// Enforce the one invariant frontmatter must never break: it cannot change whether a file is
/// packed. Checked over both context files and store nodes, since either could try it.
fn eagerness_violations(manifest: &ResolvedManifest) -> Result<Vec<DoctorError>> {
    let mut errors = Vec::new();

    for entry in &manifest.context_entries {
        // A synthesized store index has no file, and a file that cannot be read is already
        // reported as a missing context file.
        let Some(path) = entry.path() else { continue };
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        errors.extend(violations_from(
            path,
            &frontmatter::parse(&contents).0.issues,
        ));
    }

    for store in outline::outline_stores(&manifest.stores, None, None)? {
        for node in store.nodes {
            errors.extend(violations_from(&node.path, &node.issues));
        }
    }

    Ok(errors)
}

fn violations_from(path: &Path, issues: &[FrontmatterIssue]) -> Vec<DoctorError> {
    issues
        .iter()
        .filter_map(|issue| match issue {
            FrontmatterIssue::EagernessKey { key } => Some(DoctorError::FrontmatterEagerness {
                path: path.to_path_buf(),
                key: key.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn effective_composition(layers: &[DoctorStoreLayer]) -> StoreComposition {
    let layers = layers
        .iter()
        .map(|layer| ResolvedStoreLayer {
            path: layer.path.clone(),
            composition: layer.composition,
        })
        .collect::<Vec<_>>();
    resolve::effective_store_composition(&layers)
}

fn settings_report(settings: EffectiveSettings) -> DoctorSettingsReport {
    DoctorSettingsReport {
        effective: DoctorEffectiveSettings {
            allow_missing: settings.allow_missing,
            global_root: settings.global_root,
        },
        layers: settings.layers,
    }
}

impl Display for DoctorReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "healthy: {}", self.healthy)?;
        writeln!(f, "layers:")?;
        display_layers(f, &self.layers)?;
        writeln!(f, "errors:")?;
        if self.errors.is_empty() {
            writeln!(f, "  - <none>")?;
        } else {
            for error in &self.errors {
                display_error(f, error)?;
            }
        }
        writeln!(f, "warnings:")?;
        if self.warnings.is_empty() {
            writeln!(f, "  - <none>")?;
        } else {
            for warning in &self.warnings {
                display_warning(f, warning)?;
            }
        }
        Ok(())
    }
}

impl Display for DoctorNodesReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "healthy: {}", self.healthy)?;
        writeln!(f, "tiers:")?;
        if self.tier_counts.is_empty() {
            writeln!(f, "  - <none>")?;
        } else {
            for tier in &self.tier_counts {
                writeln!(f, "  - {}: {}", tier.tier.label(), tier.count)?;
            }
        }
        writeln!(f, "nodes:")?;
        if self.stores.iter().all(|store| store.nodes.is_empty()) {
            return writeln!(f, "  - <none>");
        }
        for store in &self.stores {
            for node in &store.nodes {
                writeln!(
                    f,
                    "  - {}:{} [{}] {}",
                    store.name,
                    node.reference,
                    node.tier.label(),
                    node.path.display(),
                )?;
                for issue in &node.issues {
                    writeln!(f, "    {}", issue_label(issue))?;
                }
                if let Some(reason) = &node.unreadable {
                    writeln!(f, "    error: unreadable: {reason}")?;
                }
                for path in &node.shadowed {
                    writeln!(f, "    note: shadows {}", path.display())?;
                }
            }
        }
        if self
            .stores
            .iter()
            .any(|store| !store.scan_issues.is_empty())
        {
            writeln!(f, "scan_issues:")?;
            for store in &self.stores {
                for issue in &store.scan_issues {
                    writeln!(f, "  - {}: {issue}", store.name)?;
                }
            }
        }
        Ok(())
    }
}

fn issue_label(issue: &FrontmatterIssue) -> String {
    match issue {
        FrontmatterIssue::EagernessKey { key } => format!(
            "error: frontmatter key `{key}` would affect eagerness; rata.toml owns that, not the file"
        ),
        FrontmatterIssue::MisspelledKey { key, expected } => {
            format!("warn: frontmatter key `{key}` looks like a typo of `{expected}`")
        }
        FrontmatterIssue::Unterminated => {
            "warn: frontmatter block is never closed; treated as body".to_string()
        }
        FrontmatterIssue::Malformed { line } => {
            format!("warn: unparsable frontmatter line `{line}`")
        }
    }
}

impl Display for DoctorStoresReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "stores:")?;
        if self.stores.is_empty() {
            return writeln!(f, "  - <none>");
        }
        for store in &self.stores {
            writeln!(
                f,
                "  - {} [{}]",
                store.name,
                composition_label(store.composition)
            )?;
            for layer in &store.layers {
                writeln!(
                    f,
                    "    - {} {} => {} (composition: {})",
                    layer.scope_kind,
                    layer.scope_root.display(),
                    layer.path.display(),
                    layer
                        .composition
                        .map(composition_label)
                        .unwrap_or("<inherited>"),
                )?;
            }
        }
        Ok(())
    }
}

impl Display for DoctorSettingsReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "effective:")?;
        writeln!(f, "  allow_missing: {}", self.effective.allow_missing)?;
        writeln!(f, "  global_root: {}", self.effective.global_root.display())?;
        writeln!(f, "layers:")?;
        if self.layers.is_empty() {
            writeln!(f, "  - <none>")?;
        } else {
            for layer in &self.layers {
                writeln!(
                    f,
                    "  - {} {} allow_missing={} global_root={}",
                    layer.scope_kind,
                    layer.scope_root.display(),
                    layer
                        .allow_missing
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "<unset>".to_string()),
                    layer
                        .global_root
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "<unset>".to_string()),
                )?;
            }
        }
        Ok(())
    }
}

fn display_layers(f: &mut fmt::Formatter<'_>, layers: &[DoctorLayer]) -> fmt::Result {
    if layers.is_empty() {
        writeln!(f, "  - <none>")?;
    } else {
        for layer in layers {
            writeln!(f, "  - {}: {}", layer.kind, layer.root.display())?;
        }
    }
    Ok(())
}

fn display_error(f: &mut fmt::Formatter<'_>, error: &DoctorError) -> fmt::Result {
    match error {
        DoctorError::RemoteFile {
            name,
            scope_kind,
            scope_root,
            destination,
            status,
            detail,
        } => {
            writeln!(
                f,
                "  - remote {name} [{scope_kind} {}] {} => {}",
                scope_root.display(),
                status_label(status),
                destination.display(),
            )?;
            if let Some(detail) = detail {
                writeln!(f, "    detail: {detail}")?;
            }
        }
        DoctorError::MissingContextFile {
            path,
            scope_kind,
            scope_root,
            source,
        } => writeln!(
            f,
            "  - missing context {} [{} {} {}]",
            path.display(),
            scope_kind,
            scope_root.display(),
            source_label(source),
        )?,
        DoctorError::FrontmatterEagerness { path, key } => writeln!(
            f,
            "  - frontmatter key `{key}` in {} would affect eagerness; rata.toml owns that, not the file",
            path.display(),
        )?,
        DoctorError::UnknownContextStore {
            store,
            scope_kind,
            scope_root,
        } => writeln!(
            f,
            "  - unknown store `{store}:` in [context].include [{scope_kind} {}]",
            scope_root.display(),
        )?,
    }
    Ok(())
}

fn display_warning(f: &mut fmt::Formatter<'_>, warning: &DoctorWarning) -> fmt::Result {
    match warning {
        DoctorWarning::HandMaintainedIndex { path, store, links } => writeln!(
            f,
            "  - {} looks like a hand-maintained index of {store} ({} sibling links); replace it with the `{store}:` store ref in [context].include",
            path.display(),
            links.len(),
        ),
    }
}

fn composition_label(composition: StoreComposition) -> &'static str {
    match composition {
        StoreComposition::Replace => "replace",
        StoreComposition::GlobalFirst => "global-first",
        StoreComposition::LocalFirst => "local-first",
    }
}

fn status_label(status: &RemoteStatusKind) -> &'static str {
    match status {
        RemoteStatusKind::Present => "present",
        RemoteStatusKind::Fetched => "fetched",
        RemoteStatusKind::Refetched => "refetched",
        RemoteStatusKind::Missing => "missing",
        RemoteStatusKind::FetchFailed => "fetch_failed",
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
    use crate::test_support::temp_dir;
    use std::fs;
    use std::path::Path;

    use crate::config::StoreComposition;

    use super::{DoctorError, DoctorWarning, run_doctor, run_settings_doctor, run_stores_doctor};

    #[test]
    fn doctor_subcommands_expose_detailed_store_and_settings_diagnostics() {
        let root = temp_dir("doctor-subcommands");
        let global_root = root.join("global");
        let local_root = root.join("project");
        write_config(
            &global_root,
            r#"
version = 1

[settings]
allow_missing = false

[stores]
skills = { path = "stores/skills", composition = "global-first" }
"#,
        );
        write_config(
            &local_root,
            r#"
version = 1

[settings]
allow_missing = true

[stores]
skills = { path = ".rata/stores/skills" }
"#,
        );

        let report = run_doctor(&local_root, Some(&global_root), &[]).unwrap();
        assert!(report.healthy);
        assert!(report.errors.is_empty());
        assert_eq!(report.layers.len(), 2);

        let stores = run_stores_doctor(&local_root, Some(&global_root), &[]).unwrap();
        assert_eq!(stores.stores.len(), 1);
        assert_eq!(stores.stores[0].name, "skills");
        assert_eq!(stores.stores[0].composition, StoreComposition::GlobalFirst);
        assert_eq!(stores.stores[0].layers.len(), 2);
        assert_eq!(stores.stores[0].layers[1].composition, None);

        let settings = run_settings_doctor(&local_root, Some(&global_root), &[]).unwrap();
        assert!(settings.effective.allow_missing);
        assert_eq!(settings.layers.len(), 2);
        assert_eq!(settings.layers[0].allow_missing, Some(false));
        assert_eq!(settings.layers[1].allow_missing, Some(true));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn doctor_rejects_frontmatter_that_would_decide_its_own_eagerness() {
        let root = temp_dir("doctor-eagerness");
        let global_root = root.join("global");
        fs::create_dir_all(global_root.join("memory")).unwrap();
        write_config(
            &global_root,
            r#"
version = 1

[context]
include = ["AGENTS.md"]

[stores]
memory = "memory"
"#,
        );
        fs::write(global_root.join("AGENTS.md"), "# Agents\n\nProse.\n").unwrap();
        // Self-description is fine.
        fs::write(
            global_root.join("memory/allowed.md"),
            "---\ndescription: A memory\ntags: [nix]\n---\n# Allowed\n",
        )
        .unwrap();

        let report = run_doctor(&global_root, Some(&global_root), &[]).unwrap();
        assert!(report.healthy, "{:?}", report.errors);

        // Opting itself into a profile is not.
        fs::write(
            global_root.join("memory/sneaky.md"),
            "---\ndescription: A memory\nprofile: build\n---\n# Sneaky\n",
        )
        .unwrap();

        let report = run_doctor(&global_root, Some(&global_root), &[]).unwrap();
        assert!(!report.healthy);
        assert!(report.errors.iter().any(|error| matches!(
            error,
            DoctorError::FrontmatterEagerness { key, path }
                if key == "profile" && path.ends_with("sneaky.md")
        )));

        // A context file is held to the same rule.
        fs::write(
            global_root.join("AGENTS.md"),
            "---\ninclude: [everything.md]\n---\n# Agents\n",
        )
        .unwrap();
        let report = run_doctor(&global_root, Some(&global_root), &[]).unwrap();
        assert_eq!(
            report
                .errors
                .iter()
                .filter(|error| matches!(error, DoctorError::FrontmatterEagerness { .. }))
                .count(),
            2
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn doctor_warns_about_a_file_that_hand_maintains_an_index_of_its_siblings() {
        let root = temp_dir("doctor-hand-index");
        let global_root = root.join("global");
        fs::create_dir_all(global_root.join("memory")).unwrap();
        write_config(
            &global_root,
            "version = 1\n\n[stores]\nmemory = \"memory\"\n",
        );
        fs::write(global_root.join("memory/nix.md"), "# Nix\n").unwrap();
        fs::write(global_root.join("memory/containers.md"), "# Containers\n").unwrap();
        // One cross-reference is prose, not an index.
        fs::write(
            global_root.join("memory/tools.md"),
            "# Tools\n\nSee [nix](nix.md) for the pinning pattern.\n",
        )
        .unwrap();

        let report = run_doctor(&global_root, Some(&global_root), &[]).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);

        // A pointer list of the whole store is.
        fs::write(
            global_root.join("memory/MEMORY.md"),
            "# Memory\n\n- [Nix](nix.md)\n- [Containers](containers.md)\n- [Tools](tools.md)\n",
        )
        .unwrap();

        let report = run_doctor(&global_root, Some(&global_root), &[]).unwrap();
        assert!(report.healthy, "an index is a warning, not an error");
        assert!(report.warnings.iter().any(|warning| matches!(
            warning,
            DoctorWarning::HandMaintainedIndex { path, links, .. }
                if path.ends_with("MEMORY.md") && links.len() == 3
        )));

        fs::remove_dir_all(root).unwrap();
    }

    fn write_config(root: &Path, contents: &str) {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("rata.toml"), contents).unwrap();
    }
}
