use std::fmt::{self, Display};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::resolve::{self, MissingContextFile};
use crate::{config::RemoteStatusKind, errors::Result};

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub cwd: PathBuf,
    pub global_root: Option<PathBuf>,
    pub local_root: Option<PathBuf>,
    pub local_roots: Vec<PathBuf>,
    pub selected_profiles: Vec<String>,
    pub allow_missing: bool,
    pub settings_layers: Vec<crate::config::SettingsLayer>,
    pub remote_files: Vec<crate::config::RemoteFileStatus>,
    pub missing_context_files: Vec<MissingContextFile>,
    pub healthy: bool,
}

pub fn run_doctor(
    cwd: &Path,
    global_root_override: Option<&Path>,
    selected_profiles: &[String],
) -> Result<DoctorReport> {
    let manifest = resolve::resolve_manifest(cwd, global_root_override, selected_profiles)?;
    let healthy = manifest.missing_context_files.is_empty()
        && manifest.remote_files.iter().all(|remote| {
            !matches!(
                remote.status,
                RemoteStatusKind::Missing | RemoteStatusKind::FetchFailed
            )
        });

    Ok(DoctorReport {
        cwd: manifest.cwd,
        global_root: manifest.global_root,
        local_root: manifest.local_root,
        local_roots: manifest.local_roots,
        selected_profiles: manifest.selected_profiles,
        allow_missing: manifest.settings.allow_missing,
        settings_layers: manifest.settings.layers,
        remote_files: manifest.remote_files,
        missing_context_files: manifest.missing_context_files,
        healthy,
    })
}

impl Display for DoctorReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "cwd: {}", self.cwd.display())?;
        writeln!(
            f,
            "global_root: {}",
            self.global_root
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<none>".to_string())
        )?;
        writeln!(
            f,
            "local_root: {}",
            self.local_root
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<none>".to_string())
        )?;
        writeln!(f, "healthy: {}", self.healthy)?;
        writeln!(f, "allow_missing: {}", self.allow_missing)?;
        writeln!(f, "selected_profiles:")?;
        if self.selected_profiles.is_empty() {
            writeln!(f, "  - <none>")?;
        } else {
            for profile in &self.selected_profiles {
                writeln!(f, "  - {profile}")?;
            }
        }
        writeln!(f, "settings_layers:")?;
        if self.settings_layers.is_empty() {
            writeln!(f, "  - <none>")?;
        } else {
            for layer in &self.settings_layers {
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
        writeln!(f, "remote_files:")?;
        if self.remote_files.is_empty() {
            writeln!(f, "  - <none>")?;
        } else {
            for remote in &self.remote_files {
                writeln!(
                    f,
                    "  - {} [{}] {} => {}",
                    remote.name,
                    remote.scope_kind,
                    status_label(&remote.status),
                    remote.destination.display(),
                )?;
                if let Some(detail) = &remote.detail {
                    writeln!(f, "    detail: {detail}")?;
                }
            }
        }
        writeln!(f, "missing_context_files:")?;
        if self.missing_context_files.is_empty() {
            writeln!(f, "  - <none>")?;
        } else {
            for missing in &self.missing_context_files {
                writeln!(
                    f,
                    "  - {} [{} {} {}]",
                    missing.path.display(),
                    missing.scope_kind,
                    missing.scope_root.display(),
                    source_label(&missing.source),
                )?;
            }
        }

        Ok(())
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

fn source_label(source: &crate::resolve::ContextSource) -> String {
    match source {
        crate::resolve::ContextSource::Base => "base".to_string(),
        crate::resolve::ContextSource::Profile { name } => format!("profile:{name}"),
    }
}
