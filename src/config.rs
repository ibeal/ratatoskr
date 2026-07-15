use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::{RatatoskrError, Result};

pub const LOCAL_DIR: &str = ".ratatoskr";
pub const CONFIG_FILE: &str = "ratatoskr.toml";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ScopeConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub context: ContextConfig,
    #[serde(default)]
    pub stores: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ContextConfig {
    #[serde(default)]
    pub include: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LoadedScope {
    pub kind: ScopeKind,
    pub root: PathBuf,
    pub config: ScopeConfig,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ScopeKind {
    Global,
    Local,
}

impl ScopeKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Local => "local",
        }
    }
}

pub fn default_global_root() -> PathBuf {
    home_dir().join(".config").join("ratatoskr")
}

pub fn discover_local_root(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let candidate = dir.join(LOCAL_DIR);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }

    None
}

pub fn load_global_scope() -> Result<Option<LoadedScope>> {
    load_scope(default_global_root(), ScopeKind::Global)
}

pub fn load_local_scope(start: &Path) -> Result<Option<LoadedScope>> {
    match discover_local_root(start) {
        Some(root) => load_scope(root, ScopeKind::Local),
        None => Ok(None),
    }
}

pub fn load_scope(root: PathBuf, kind: ScopeKind) -> Result<Option<LoadedScope>> {
    if !root.exists() {
        return Ok(None);
    }

    let config = load_scope_config(&root)?;
    Ok(Some(LoadedScope { kind, root, config }))
}

pub fn load_scope_config(root: &Path) -> Result<ScopeConfig> {
    let path = root.join(CONFIG_FILE);
    let raw = fs::read_to_string(&path)
        .map_err(|source| RatatoskrError::ReadConfig(path.clone(), source))?;
    toml::from_str(&raw).map_err(|source| RatatoskrError::ParseConfig(path, source))
}

pub fn validate_scope_root(root: &Path, kind: ScopeKind) -> Result<()> {
    if kind == ScopeKind::Local && root.file_name().and_then(|s| s.to_str()) != Some(LOCAL_DIR) {
        return Err(RatatoskrError::InvalidRoot(format!(
            "local roots must end with `{LOCAL_DIR}`: {}",
            root.display()
        )));
    }

    Ok(())
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn default_version() -> u32 {
    1
}
