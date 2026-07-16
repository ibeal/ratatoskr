use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::{self, LoadedScope};
use crate::errors::{RatatoskrError, Result};

#[derive(Debug, Serialize)]
pub struct ResolvedManifest {
    pub cwd: PathBuf,
    pub global_root: Option<PathBuf>,
    pub local_root: Option<PathBuf>,
    pub local_roots: Vec<PathBuf>,
    pub settings: EffectiveSettings,
    pub selected_profiles: Vec<String>,
    pub available_profiles: Vec<AvailableProfile>,
    pub scopes: Vec<ResolvedScope>,
    pub context_files: Vec<PathBuf>,
    pub context_entries: Vec<ResolvedContextEntry>,
    pub stores: BTreeMap<String, PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct ResolvedStores {
    pub cwd: PathBuf,
    pub global_root: Option<PathBuf>,
    pub local_root: Option<PathBuf>,
    pub local_roots: Vec<PathBuf>,
    pub settings: EffectiveSettings,
    pub stores: BTreeMap<String, PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct EffectiveSettings {
    pub allow_missing: bool,
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
    pub stores: BTreeMap<String, PathBuf>,
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

#[derive(Clone, Debug, Serialize)]
pub struct ResolvedContextEntry {
    pub path: PathBuf,
    pub scope_kind: String,
    pub scope_root: PathBuf,
    pub source: ContextSource,
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
    let locals = config::load_local_scopes(cwd)?;
    let global = config::load_global_scope(global_root_override, &locals)?;
    let allow_missing = config::effective_allow_missing(global.as_ref(), &locals);

    if let Some(scope) = global.as_ref() {
        config::prepare_remote_files(scope);
    }
    for scope in &locals {
        config::prepare_remote_files(scope);
    }

    let mut scopes = Vec::new();
    let mut context_files = Vec::new();
    let mut context_entries = Vec::new();
    let mut stores = BTreeMap::new();
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
        for (name, path) in &resolved.stores {
            stores.insert(name.clone(), path.clone());
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
        for (name, path) in &resolved.stores {
            stores.insert(name.clone(), path.clone());
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

    Ok(ResolvedManifest {
        cwd: cwd.to_path_buf(),
        global_root: global.map(|scope| scope.root),
        local_root: locals.last().map(|scope| scope.root.clone()),
        local_roots: locals.iter().map(|scope| scope.root.clone()).collect(),
        settings: EffectiveSettings { allow_missing },
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
    })
}

fn resolve_scope(
    scope: &LoadedScope,
    selected_profiles: &[String],
    matched_profiles: &mut BTreeSet<String>,
) -> ResolvedScope {
    let base_context_files = scope
        .config
        .context
        .include
        .iter()
        .map(|entry| scope.root.join(entry))
        .collect::<Vec<_>>();

    let mut context_files = base_context_files.clone();
    let mut context_entries = base_context_files
        .iter()
        .cloned()
        .map(|path| ResolvedContextEntry {
            path,
            scope_kind: scope.kind.label().to_string(),
            scope_root: scope.root.clone(),
            source: ContextSource::Base,
        })
        .collect::<Vec<_>>();
    let mut active_profiles = Vec::new();

    for profile_name in selected_profiles {
        if let Some(profile) = scope.config.profiles.get(profile_name) {
            matched_profiles.insert(profile_name.clone());
            let profile_files = profile
                .include
                .iter()
                .map(|entry| scope.root.join(entry))
                .collect::<Vec<_>>();
            push_unique_paths(&mut context_files, profile_files.iter().cloned());
            push_unique_entries(
                &mut context_entries,
                profile_files
                    .iter()
                    .cloned()
                    .map(|path| ResolvedContextEntry {
                        path,
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
            context_files: profile
                .include
                .iter()
                .map(|entry| scope.root.join(entry))
                .collect(),
        })
        .collect();

    let stores = scope
        .config
        .stores
        .iter()
        .map(|(name, relative_path)| (name.clone(), scope.root.join(relative_path)))
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
        writeln!(f, "allow_missing: {}", self.settings.allow_missing)?;
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
        for (name, path) in &self.stores {
            writeln!(f, "  - {} => {}", name, path.display())?;
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
        writeln!(f, "allow_missing: {}", self.settings.allow_missing)?;
        writeln!(f, "local_roots:")?;
        if self.local_roots.is_empty() {
            writeln!(f, "  - <none>")?;
        } else {
            for root in &self.local_roots {
                writeln!(f, "  - {}", root.display())?;
            }
        }
        writeln!(f, "stores:")?;
        for (name, path) in &self.stores {
            writeln!(f, "  - {} => {}", name, path.display())?;
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
        settings: manifest.settings,
        stores: manifest.stores,
    })
}

fn display_path(path: Option<&PathBuf>) -> String {
    match path {
        Some(path) => path.display().to_string(),
        None => "<none>".to_string(),
    }
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
        if !target.iter().any(|existing| existing.path == entry.path) {
            target.push(entry);
        }
    }
}
