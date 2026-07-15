use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::{RatatoskrError, Result};

pub const LOCAL_DIR: &str = ".rata";
pub const CONFIG_FILE: &str = ".rata.toml";
pub const GLOBAL_ROOT_ENV: &str = "RATA_ROOT";
pub const GLOBAL_ROOT_POINTER_DIR: &str = ".rata";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ScopeConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub context: ContextConfig,
    #[serde(default)]
    pub profiles: BTreeMap<String, ProfileConfig>,
    #[serde(default)]
    pub stores: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ContextConfig {
    #[serde(default)]
    pub include: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProfileConfig {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub include: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct GlobalRootPointerConfig {
    root: Option<String>,
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
    home_dir().join(".config").join("rata")
}

pub fn resolve_global_root(cli_override: Option<&Path>) -> PathBuf {
    if let Some(path) = cli_override {
        return path.to_path_buf();
    }

    if let Some(path) = std::env::var_os(GLOBAL_ROOT_ENV) {
        return PathBuf::from(path);
    }

    if let Some(path) = load_global_root_pointer() {
        return path;
    }

    default_global_root()
}

pub fn discover_local_roots(start: &Path) -> Vec<PathBuf> {
    let mut roots = start
        .ancestors()
        .filter_map(|dir| {
            let candidate = dir.join(LOCAL_DIR);
            candidate.is_dir().then_some(candidate)
        })
        .collect::<Vec<_>>();
    roots.reverse();
    roots
}

pub fn load_global_scope(root_override: Option<&Path>) -> Result<Option<LoadedScope>> {
    load_scope(resolve_global_root(root_override), ScopeKind::Global)
}

pub fn load_local_scopes(start: &Path) -> Result<Vec<LoadedScope>> {
    let mut scopes = Vec::new();

    for root in discover_local_roots(start) {
        if let Some(scope) = load_scope(root, ScopeKind::Local)? {
            scopes.push(scope);
        }
    }

    Ok(scopes)
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

pub fn global_root_pointer_path() -> PathBuf {
    home_dir()
        .join(".config")
        .join(GLOBAL_ROOT_POINTER_DIR)
        .join(CONFIG_FILE)
}

fn load_global_root_pointer() -> Option<PathBuf> {
    let pointer_path = global_root_pointer_path();
    if !pointer_path.is_file() {
        return None;
    }

    let raw = fs::read_to_string(&pointer_path).ok()?;
    let config = toml::from_str::<GlobalRootPointerConfig>(&raw).ok()?;
    let root = config.root?;
    let root = root.trim();
    if root.is_empty() {
        return None;
    }

    Some(PathBuf::from(root))
}

fn default_version() -> u32 {
    1
}
