use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::{self, LoadedScope};
use crate::errors::Result;

#[derive(Debug, Serialize)]
pub struct ResolvedManifest {
    pub cwd: PathBuf,
    pub global_root: Option<PathBuf>,
    pub local_root: Option<PathBuf>,
    pub scopes: Vec<ResolvedScope>,
    pub context_files: Vec<PathBuf>,
    pub stores: BTreeMap<String, PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct ResolvedScope {
    pub kind: String,
    pub root: PathBuf,
    pub context_files: Vec<PathBuf>,
    pub stores: BTreeMap<String, PathBuf>,
}

pub fn resolve_manifest(cwd: &Path) -> Result<ResolvedManifest> {
    let global = config::load_global_scope()?;
    let local = config::load_local_scope(cwd)?;

    let mut scopes = Vec::new();
    let mut context_files = Vec::new();
    let mut stores = BTreeMap::new();

    for scope in [global.as_ref(), local.as_ref()].into_iter().flatten() {
        let resolved = resolve_scope(scope);
        context_files.extend(resolved.context_files.iter().cloned());
        for (name, path) in &resolved.stores {
            stores.insert(name.clone(), path.clone());
        }
        scopes.push(resolved);
    }

    Ok(ResolvedManifest {
        cwd: cwd.to_path_buf(),
        global_root: global.map(|scope| scope.root),
        local_root: local.map(|scope| scope.root),
        scopes,
        context_files,
        stores,
    })
}

fn resolve_scope(scope: &LoadedScope) -> ResolvedScope {
    let context_files = scope
        .config
        .context
        .include
        .iter()
        .map(|entry| scope.root.join(entry))
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
        context_files,
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

fn display_path(path: Option<&PathBuf>) -> String {
    match path {
        Some(path) => path.display().to_string(),
        None => "<none>".to_string(),
    }
}
