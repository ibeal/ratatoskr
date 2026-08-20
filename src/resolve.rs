use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::{self, LoadedScope, RemoteFileStatus, StoreComposition};
use crate::errors::{RatatoskrError, Result};

#[derive(Debug, Serialize)]
pub struct ResolvedManifest {
    pub cwd: PathBuf,
    pub global_root: Option<PathBuf>,
    pub local_root: Option<PathBuf>,
    pub local_roots: Vec<PathBuf>,
    #[serde(skip_serializing)]
    pub settings: config::EffectiveSettings,
    pub selected_profiles: Vec<String>,
    pub available_profiles: Vec<AvailableProfile>,
    pub scopes: Vec<ResolvedScope>,
    pub context_files: Vec<PathBuf>,
    pub context_entries: Vec<ResolvedContextEntry>,
    pub stores: BTreeMap<String, ResolvedStore>,
    pub remote_files: Vec<RemoteFileStatus>,
}

#[derive(Debug, Serialize)]
pub struct ResolvedStores {
    pub cwd: PathBuf,
    pub global_root: Option<PathBuf>,
    pub local_root: Option<PathBuf>,
    pub local_roots: Vec<PathBuf>,
    pub stores: BTreeMap<String, ResolvedStore>,
}

#[derive(Debug, Serialize)]
pub struct AvailableProfile {
    pub name: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ResolvedScope {
    pub kind: String,
    pub root: PathBuf,
    pub base_context_files: Vec<PathBuf>,
    pub active_profiles: Vec<AppliedProfile>,
    pub context_files: Vec<PathBuf>,
    pub context_entries: Vec<ResolvedContextEntry>,
    pub available_profiles: Vec<ScopeProfile>,
    pub stores: BTreeMap<String, ResolvedStoreLayer>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ResolvedStoreLayer {
    pub path: PathBuf,
    #[serde(skip_serializing)]
    pub composition: Option<StoreComposition>,
}

#[derive(Debug, Serialize)]
pub struct ResolvedStore {
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct AppliedProfile {
    pub name: String,
    pub context_files: Vec<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct ScopeProfile {
    pub name: String,
    pub description: Option<String>,
    pub context_files: Vec<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct MissingContextFile {
    pub path: PathBuf,
    pub scope_kind: String,
    pub scope_root: PathBuf,
    pub source: ContextSource,
}

#[derive(Debug, Serialize)]
pub struct UnknownContextStore {
    pub store: String,
    pub scope_kind: String,
    pub scope_root: PathBuf,
    pub source: ContextSource,
}

#[derive(Debug)]
pub struct ManifestInspection {
    pub manifest: ResolvedManifest,
    pub missing_context_files: Vec<MissingContextFile>,
    pub unknown_context_stores: Vec<UnknownContextStore>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ResolvedContextEntry {
    #[serde(flatten)]
    pub target: ContextTarget,
    pub scope_kind: String,
    pub scope_root: PathBuf,
    pub source: ContextSource,
}

/// What an `[context].include` entry resolves to. A plain path is a file to read; `<store>:` is a
/// request for that store's computed outline, which has no file behind it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "target_kind", rename_all = "snake_case")]
pub enum ContextTarget {
    File { path: PathBuf },
    StoreIndex { store: String },
}

impl ContextTarget {
    /// Parse one `[context].include` entry. `memory:` is a store ref; anything else is a path
    /// relative to the scope root.
    pub fn parse(root: &Path, entry: &str) -> Self {
        match store_ref(entry) {
            Some(store) => Self::StoreIndex {
                store: store.to_string(),
            },
            None => Self::File {
                path: root.join(entry),
            },
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::File { path } => Some(path),
            Self::StoreIndex { .. } => None,
        }
    }

    /// How the entry is named in human-readable output.
    pub fn label(&self) -> String {
        match self {
            Self::File { path } => path.display().to_string(),
            Self::StoreIndex { store } => format!("{store}:"),
        }
    }
}

/// A store ref is a bare store name followed by a colon and nothing else, e.g. `memory:`. The
/// trailing colon is what distinguishes it from a relative path.
fn store_ref(entry: &str) -> Option<&str> {
    let name = entry.strip_suffix(':')?;
    let valid = !name.is_empty()
        && !name.contains('/')
        && !name.contains(':')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then_some(name)
}

impl ResolvedContextEntry {
    pub fn path(&self) -> Option<&Path> {
        self.target.path()
    }

    fn is_missing_file(&self) -> bool {
        self.path().is_some_and(is_missing_file)
    }

    fn unknown_store(&self, stores: &BTreeMap<String, ResolvedStore>) -> bool {
        match &self.target {
            ContextTarget::StoreIndex { store } => !stores.contains_key(store),
            ContextTarget::File { .. } => false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextSource {
    Base,
    Profile { name: String },
}

pub fn resolve_manifest(
    cwd: &Path,
    global_root_override: Option<&Path>,
    selected_profiles: &[String],
) -> Result<ResolvedManifest> {
    Ok(inspect_manifest(cwd, global_root_override, selected_profiles)?.manifest)
}

pub fn inspect_manifest(
    cwd: &Path,
    global_root_override: Option<&Path>,
    selected_profiles: &[String],
) -> Result<ManifestInspection> {
    let locals = config::load_local_scopes(cwd)?;
    let global = config::load_global_scope(global_root_override, &locals)?;
    let settings =
        config::resolve_effective_settings(global.as_ref(), &locals, global_root_override);

    let mut remote_files = Vec::new();
    if let Some(scope) = global.as_ref() {
        remote_files.extend(config::prepare_remote_files(scope));
    }
    for scope in &locals {
        remote_files.extend(config::prepare_remote_files(scope));
    }

    let mut scopes = Vec::new();
    let mut context_files = Vec::new();
    let mut context_entries = Vec::new();
    let mut store_layers = BTreeMap::<String, Vec<ResolvedStoreLayer>>::new();
    let mut available_profiles = BTreeMap::<String, BTreeSet<String>>::new();
    let mut matched_profiles = BTreeSet::new();

    if let Some(scope) = global.as_ref() {
        let resolved = resolve_scope(scope, selected_profiles, &mut matched_profiles);

        for profile in &resolved.available_profiles {
            available_profiles
                .entry(profile.name.clone())
                .or_default()
                .insert(resolved.kind.clone());
        }

        push_unique_paths(&mut context_files, resolved.context_files.iter().cloned());
        push_unique_entries(
            &mut context_entries,
            resolved.context_entries.iter().cloned(),
        );
        for (name, store) in &resolved.stores {
            store_layers
                .entry(name.clone())
                .or_default()
                .push(store.clone());
        }
        scopes.push(resolved);
    }

    for scope in &locals {
        let resolved = resolve_scope(scope, selected_profiles, &mut matched_profiles);

        for profile in &resolved.available_profiles {
            available_profiles
                .entry(profile.name.clone())
                .or_default()
                .insert(resolved.kind.clone());
        }

        push_unique_paths(&mut context_files, resolved.context_files.iter().cloned());
        push_unique_entries(
            &mut context_entries,
            resolved.context_entries.iter().cloned(),
        );
        for (name, store) in &resolved.stores {
            store_layers
                .entry(name.clone())
                .or_default()
                .push(store.clone());
        }
        scopes.push(resolved);
    }

    let missing_profiles = selected_profiles
        .iter()
        .filter(|profile| !matched_profiles.contains(profile.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !missing_profiles.is_empty() {
        return Err(RatatoskrError::UnknownProfiles(missing_profiles));
    }

    let missing_context_files = context_entries
        .iter()
        .filter(|entry| entry.is_missing_file())
        .cloned()
        .map(|entry| MissingContextFile {
            path: entry.path().unwrap_or(Path::new("")).to_path_buf(),
            scope_kind: entry.scope_kind,
            scope_root: entry.scope_root,
            source: entry.source,
        })
        .collect();

    let stores = compose_stores(store_layers);

    // A store ref naming a store that no scope declares is as absent as a missing file, and is
    // treated the same way: reported by `doctor`, and only fatal when allow_missing is false.
    let unknown_context_stores = context_entries
        .iter()
        .filter(|entry| entry.unknown_store(&stores))
        .cloned()
        .filter_map(|entry| match entry.target {
            ContextTarget::StoreIndex { store } => Some(UnknownContextStore {
                store,
                scope_kind: entry.scope_kind,
                scope_root: entry.scope_root,
                source: entry.source,
            }),
            ContextTarget::File { .. } => None,
        })
        .collect();

    if settings.allow_missing {
        context_entries.retain(|entry| !entry.is_missing_file() && !entry.unknown_store(&stores));
        context_files.retain(|path| !is_missing_file(path));
        for scope in &mut scopes {
            scope
                .base_context_files
                .retain(|path| !is_missing_file(path));
            scope.context_files.retain(|path| !is_missing_file(path));
            scope
                .context_entries
                .retain(|entry| !entry.is_missing_file() && !entry.unknown_store(&stores));
            for profile in &mut scope.active_profiles {
                profile.context_files.retain(|path| !is_missing_file(path));
            }
            for profile in &mut scope.available_profiles {
                profile.context_files.retain(|path| !is_missing_file(path));
            }
        }
    }

    Ok(ManifestInspection {
        manifest: ResolvedManifest {
            cwd: cwd.to_path_buf(),
            global_root: Some(settings.global_root.clone()),
            local_root: locals.last().map(|scope| scope.root.clone()),
            local_roots: locals.iter().map(|scope| scope.root.clone()).collect(),
            settings,
            selected_profiles: selected_profiles.to_vec(),
            available_profiles: available_profiles
                .into_iter()
                .map(|(name, scopes)| AvailableProfile {
                    name,
                    scopes: scopes.into_iter().collect(),
                })
                .collect(),
            scopes,
            context_files,
            context_entries,
            stores,
            remote_files,
        },
        missing_context_files,
        unknown_context_stores,
    })
}

fn resolve_scope(
    scope: &LoadedScope,
    selected_profiles: &[String],
    matched_profiles: &mut BTreeSet<String>,
) -> ResolvedScope {
    let base_targets = resolve_targets(scope, &scope.config.context.include);
    let base_context_files = file_paths(&base_targets);

    let mut context_files = base_context_files.clone();
    let mut context_entries = base_targets
        .iter()
        .cloned()
        .map(|target| ResolvedContextEntry {
            target,
            scope_kind: scope.kind.label().to_string(),
            scope_root: scope.root.clone(),
            source: ContextSource::Base,
        })
        .collect::<Vec<_>>();
    let mut active_profiles = Vec::new();

    for profile_name in selected_profiles {
        if let Some(profile) = scope.config.profiles.get(profile_name) {
            matched_profiles.insert(profile_name.clone());
            let profile_targets = resolve_targets(scope, &profile.include);
            let profile_files = file_paths(&profile_targets);
            push_unique_paths(&mut context_files, profile_files.iter().cloned());
            push_unique_entries(
                &mut context_entries,
                profile_targets
                    .into_iter()
                    .map(|target| ResolvedContextEntry {
                        target,
                        scope_kind: scope.kind.label().to_string(),
                        scope_root: scope.root.clone(),
                        source: ContextSource::Profile {
                            name: profile_name.clone(),
                        },
                    }),
            );
            active_profiles.push(AppliedProfile {
                name: profile_name.clone(),
                context_files: profile_files,
            });
        }
    }

    let available_profiles = scope
        .config
        .profiles
        .iter()
        .map(|(name, profile)| ScopeProfile {
            name: name.clone(),
            description: profile.description.clone(),
            context_files: file_paths(&resolve_targets(scope, &profile.include)),
        })
        .collect();

    let stores = scope
        .config
        .stores
        .iter()
        .map(|(name, store)| {
            (
                name.clone(),
                ResolvedStoreLayer {
                    path: scope.root.join(store.path()),
                    composition: store.composition(),
                },
            )
        })
        .collect();

    ResolvedScope {
        kind: scope.kind.label().to_string(),
        root: scope.root.clone(),
        base_context_files,
        active_profiles,
        context_files,
        context_entries,
        available_profiles,
        stores,
    }
}

impl Display for ResolvedManifest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "cwd: {}", self.cwd.display())?;
        writeln!(
            f,
            "global_root: {}",
            display_path(self.global_root.as_ref())
        )?;
        writeln!(f, "local_root: {}", display_path(self.local_root.as_ref()))?;
        writeln!(f, "local_roots:")?;
        if self.local_roots.is_empty() {
            writeln!(f, "  - <none>")?;
        } else {
            for root in &self.local_roots {
                writeln!(f, "  - {}", root.display())?;
            }
        }
        writeln!(f, "selected_profiles:")?;
        if self.selected_profiles.is_empty() {
            writeln!(f, "  - <none>")?;
        } else {
            for profile in &self.selected_profiles {
                writeln!(f, "  - {profile}")?;
            }
        }
        writeln!(f, "available_profiles:")?;
        if self.available_profiles.is_empty() {
            writeln!(f, "  - <none>")?;
        } else {
            for profile in &self.available_profiles {
                writeln!(f, "  - {} [{}]", profile.name, profile.scopes.join(", "))?;
            }
        }
        writeln!(f, "scopes:")?;
        for scope in &self.scopes {
            writeln!(f, "  - {}: {}", scope.kind, scope.root.display())?;
        }
        writeln!(f, "context_files:")?;
        for path in &self.context_files {
            writeln!(f, "  - {}", path.display())?;
        }
        writeln!(f, "stores:")?;
        for (name, store) in &self.stores {
            writeln!(f, "  - {name}")?;
            for path in &store.paths {
                writeln!(f, "    - {}", path.display())?;
            }
        }
        writeln!(f, "remote_files:")?;
        if self.remote_files.is_empty() {
            writeln!(f, "  - <none>")?;
        } else {
            for remote in &self.remote_files {
                writeln!(
                    f,
                    "  - {} [{}] {}",
                    remote.name,
                    remote.scope_kind,
                    remote_status_label(remote),
                )?;
            }
        }
        Ok(())
    }
}

impl Display for ResolvedStores {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "cwd: {}", self.cwd.display())?;
        writeln!(
            f,
            "global_root: {}",
            display_path(self.global_root.as_ref())
        )?;
        writeln!(f, "local_root: {}", display_path(self.local_root.as_ref()))?;
        writeln!(f, "local_roots:")?;
        if self.local_roots.is_empty() {
            writeln!(f, "  - <none>")?;
        } else {
            for root in &self.local_roots {
                writeln!(f, "  - {}", root.display())?;
            }
        }
        writeln!(f, "stores:")?;
        for (name, store) in &self.stores {
            writeln!(f, "  - {name}")?;
            for path in &store.paths {
                writeln!(f, "    - {}", path.display())?;
            }
        }

        Ok(())
    }
}

pub fn resolve_stores(
    cwd: &Path,
    global_root_override: Option<&Path>,
    selected_profiles: &[String],
) -> Result<ResolvedStores> {
    let manifest = resolve_manifest(cwd, global_root_override, selected_profiles)?;
    Ok(ResolvedStores {
        cwd: manifest.cwd,
        global_root: manifest.global_root,
        local_root: manifest.local_root,
        local_roots: manifest.local_roots,
        stores: manifest.stores,
    })
}

fn display_path(path: Option<&PathBuf>) -> String {
    match path {
        Some(path) => path.display().to_string(),
        None => "<none>".to_string(),
    }
}

fn compose_stores(
    layers: BTreeMap<String, Vec<ResolvedStoreLayer>>,
) -> BTreeMap<String, ResolvedStore> {
    layers
        .into_iter()
        .map(|(name, layers)| {
            let composition = effective_store_composition(&layers);
            let paths = match composition {
                StoreComposition::Replace => layers
                    .last()
                    .map(|layer| vec![layer.path.clone()])
                    .unwrap_or_default(),
                StoreComposition::GlobalFirst => {
                    layers.into_iter().map(|layer| layer.path).collect()
                }
                StoreComposition::LocalFirst => {
                    layers.into_iter().rev().map(|layer| layer.path).collect()
                }
            };
            (name, ResolvedStore { paths })
        })
        .collect()
}

pub fn effective_store_composition(layers: &[ResolvedStoreLayer]) -> StoreComposition {
    layers
        .iter()
        .filter_map(|layer| layer.composition)
        .last()
        .unwrap_or_default()
}

fn remote_status_label(remote: &RemoteFileStatus) -> String {
    match &remote.detail {
        Some(detail) => format!(
            "{:?} {} ({detail})",
            remote.status,
            remote.destination.display()
        ),
        None => format!("{:?} {}", remote.status, remote.destination.display()),
    }
}

fn is_missing_file(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(metadata) => !metadata.is_file(),
        Err(source) if source.kind() == ErrorKind::NotFound => true,
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{inspect_manifest, resolve_manifest, resolve_stores};

    #[test]
    fn resolve_stores_composes_layers_using_the_nearest_declaration() {
        let root = temp_dir("store-composition");
        let global_root = root.join("global");
        let outer_root = root.join("workspace");
        let project_root = outer_root.join("project");

        write_config(
            &global_root,
            r#"
version = 1

[stores]
skills = { path = "stores/skills", composition = "global-first" }
memory = { path = "stores/memory", composition = "global-first" }
tickets = "stores/tickets"
"#,
        );
        write_config(
            &outer_root,
            r#"
version = 1

[stores]
skills = { path = ".rata/stores/skills", composition = "global-first" }
memory = { path = ".rata/stores/memory" }
tickets = ".rata/stores/tickets"
"#,
        );
        write_config(
            &project_root,
            r#"
version = 1

[stores]
skills = { path = ".rata/stores/skills", composition = "local-first" }
memory = { path = ".rata/stores/memory" }
tickets = ".rata/stores/tickets"
"#,
        );

        let stores = resolve_stores(&project_root, Some(&global_root), &[]).unwrap();

        assert_eq!(
            stores.stores["skills"].paths,
            vec![
                project_root.join(".rata/stores/skills"),
                outer_root.join(".rata/stores/skills"),
                global_root.join("stores/skills"),
            ]
        );
        assert_eq!(
            stores.stores["memory"].paths,
            vec![
                global_root.join("stores/memory"),
                outer_root.join(".rata/stores/memory"),
                project_root.join(".rata/stores/memory"),
            ]
        );
        assert_eq!(
            stores.stores["tickets"].paths,
            vec![project_root.join(".rata/stores/tickets")]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_manifest_filters_missing_files_when_allowed() {
        let root = temp_dir("resolve-manifest");
        let global_root = root.join("global");
        let project_root = root.join("workspace").join("project");
        let local_scope = project_root.clone();

        fs::create_dir_all(global_root.join("context")).unwrap();
        fs::create_dir_all(local_scope.join("context")).unwrap();

        fs::write(
            global_root.join("rata.toml"),
            r#"
version = 1

[context]
include = ["context/global.md"]

[settings]
allow_missing = false
"#,
        )
        .unwrap();
        fs::write(global_root.join("context/global.md"), "global").unwrap();

        fs::write(
            local_scope.join("rata.toml"),
            r#"
version = 1

[context]
include = ["context/local.md", "context/missing.md"]

[settings]
allow_missing = true
"#,
        )
        .unwrap();
        fs::write(local_scope.join("context/local.md"), "local").unwrap();

        let manifest = resolve_manifest(&project_root, Some(&global_root), &[]).unwrap();
        let inspection = inspect_manifest(&project_root, Some(&global_root), &[]).unwrap();

        assert!(manifest.settings.allow_missing);
        assert_eq!(manifest.settings.layers.len(), 2);
        assert_eq!(manifest.settings.layers[0].allow_missing, Some(false));
        assert_eq!(manifest.settings.layers[1].allow_missing, Some(true));
        assert_eq!(manifest.context_files.len(), 2);
        assert!(
            manifest
                .context_files
                .iter()
                .all(|path| path != &local_scope.join("context/missing.md"))
        );
        assert_eq!(inspection.missing_context_files.len(), 1);
        assert_eq!(
            inspection.missing_context_files[0].path,
            local_scope.join("context/missing.md")
        );

        fs::remove_dir_all(root).unwrap();
    }

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rata-{label}-{unique}"));
        if path.exists() {
            fs::remove_dir_all(&path).unwrap();
        }
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_config(root: &Path, contents: &str) {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("rata.toml"), contents).unwrap();
    }

    #[allow(dead_code)]
    fn _assert_path(_: &Path) {}
}

fn resolve_targets(scope: &LoadedScope, entries: &[String]) -> Vec<ContextTarget> {
    entries
        .iter()
        .map(|entry| ContextTarget::parse(&scope.root, entry))
        .collect()
}

fn file_paths(targets: &[ContextTarget]) -> Vec<PathBuf> {
    targets
        .iter()
        .filter_map(|target| target.path().map(Path::to_path_buf))
        .collect()
}

fn push_unique_paths(target: &mut Vec<PathBuf>, paths: impl IntoIterator<Item = PathBuf>) {
    for path in paths {
        if !target.iter().any(|existing| existing == &path) {
            target.push(path);
        }
    }
}

fn push_unique_entries(
    target: &mut Vec<ResolvedContextEntry>,
    entries: impl IntoIterator<Item = ResolvedContextEntry>,
) {
    for entry in entries {
        if !target
            .iter()
            .any(|existing| existing.target == entry.target)
        {
            target.push(entry);
        }
    }
}
