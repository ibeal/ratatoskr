use std::fmt::{self, Display};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::errors::{RatatoskrError, Result};
use crate::resolve::{self, ResolvedManifest};

#[derive(Debug, Serialize)]
pub struct ContextBundle {
    pub cwd: PathBuf,
    pub global_root: Option<PathBuf>,
    pub local_root: Option<PathBuf>,
    pub local_roots: Vec<PathBuf>,
    pub selected_profiles: Vec<String>,
    pub files: Vec<ContextFileEntry>,
}

#[derive(Debug, Serialize)]
pub struct ContextFileEntry {
    pub path: PathBuf,
    pub contents: String,
}

pub fn build_bundle(
    cwd: &Path,
    global_root_override: Option<&Path>,
    selected_profiles: &[String],
) -> Result<ContextBundle> {
    let manifest = resolve::resolve_manifest(cwd, global_root_override, selected_profiles)?;
    bundle_from_manifest(manifest)
}

fn bundle_from_manifest(manifest: ResolvedManifest) -> Result<ContextBundle> {
    let mut files = Vec::new();

    for path in &manifest.context_files {
        let contents = fs::read_to_string(path)
            .map_err(|source| RatatoskrError::ReadContextFile(path.clone(), source))?;
        files.push(ContextFileEntry {
            path: path.clone(),
            contents,
        });
    }

    Ok(ContextBundle {
        cwd: manifest.cwd,
        global_root: manifest.global_root,
        local_root: manifest.local_root,
        local_roots: manifest.local_roots,
        selected_profiles: manifest.selected_profiles,
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
            write!(f, "{}", file.contents)?;
            if !file.contents.ends_with('\n') {
                writeln!(f)?;
            }
        }

        Ok(())
    }
}

fn display_path(path: Option<&PathBuf>) -> String {
    match path {
        Some(path) => path.display().to_string(),
        None => "<none>".to_string(),
    }
}
