use std::fmt::{self, Display};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cli::{OnlyTarget, ScopeFilter};
use crate::config::EffectiveSettings;
use crate::errors::{RatatoskrError, Result};
use crate::resolve::{self, ContextSource, ResolvedContextEntry, ResolvedManifest};

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
    pub path: PathBuf,
    pub scope_kind: String,
    pub scope_root: PathBuf,
    pub source: ContextSource,
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
            |entry| entry.path.file_name().and_then(|value| value.to_str()) == Some(name.as_str()),
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

        let Some(contents) = read_context_contents(&entry.path, allow_missing)? else {
            continue;
        };
        files.push(ContextFileEntry {
            path: entry.path.clone(),
            scope_kind: entry.scope_kind.clone(),
            scope_root: entry.scope_root.clone(),
            source: entry.source.clone(),
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
            writeln!(f, "{}. {}", index + 1, file.path.display())?;
        }

        for file in &self.files {
            writeln!(f)?;
            writeln!(f, "## File: {}", file.path.display())?;
            writeln!(f)?;
            writeln!(f, "scope_kind: {}", file.scope_kind)?;
            writeln!(f, "scope_root: {}", file.scope_root.display())?;
            writeln!(f, "source: {}", source_label(&file.source))?;
            writeln!(f)?;
            write!(f, "{}", file.contents)?;
            if !file.contents.ends_with('\n') {
                writeln!(f)?;
            }
        }

        Ok(())
    }
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
