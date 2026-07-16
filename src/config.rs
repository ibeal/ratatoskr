use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::errors::{RatatoskrError, Result};

pub const LOCAL_DIR: &str = ".rata";
pub const CONFIG_FILE: &str = ".rata.toml";
pub const GLOBAL_ROOT_ENV: &str = "RATA_ROOT";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ScopeConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub context: ContextConfig,
    #[serde(default)]
    pub profiles: BTreeMap<String, ProfileConfig>,
    #[serde(default)]
    pub remote_files: BTreeMap<String, RemoteFileConfig>,
    #[serde(default)]
    pub settings: SettingsConfig,
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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RemoteFileConfig {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub ttl: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SettingsConfig {
    #[serde(default)]
    pub allow_missing: Option<bool>,
    #[serde(default)]
    pub global_root: Option<String>,
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

pub fn resolve_global_root(cli_override: Option<&Path>, local_scopes: &[LoadedScope]) -> PathBuf {
    if let Some(path) = cli_override {
        return follow_global_root_jump(path.to_path_buf());
    }

    if let Some(path) = std::env::var_os(GLOBAL_ROOT_ENV) {
        return follow_global_root_jump(PathBuf::from(path));
    }

    if let Some(scope) = local_scopes
        .iter()
        .rev()
        .find(|scope| scope.config.settings.global_root.is_some())
    {
        if let Some(path) = resolve_scope_global_root(scope) {
            return follow_global_root_jump(path);
        }
    }

    follow_global_root_jump(default_global_root())
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

pub fn load_global_scope(
    root_override: Option<&Path>,
    local_scopes: &[LoadedScope],
) -> Result<Option<LoadedScope>> {
    load_scope(
        resolve_global_root(root_override, local_scopes),
        ScopeKind::Global,
    )
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

pub fn effective_allow_missing(global: Option<&LoadedScope>, locals: &[LoadedScope]) -> bool {
    let mut allow_missing = true;

    if let Some(scope) = global {
        if let Some(value) = scope.config.settings.allow_missing {
            allow_missing = value;
        }
    }

    for scope in locals {
        if let Some(value) = scope.config.settings.allow_missing {
            allow_missing = value;
        }
    }

    allow_missing
}

pub fn prepare_remote_files(scope: &LoadedScope) {
    for remote in scope.config.remote_files.values() {
        prepare_remote_file(scope, remote);
    }
}

fn default_version() -> u32 {
    1
}

fn resolve_scope_global_root(scope: &LoadedScope) -> Option<PathBuf> {
    scope
        .config
        .settings
        .global_root
        .as_deref()
        .map(|value| resolve_path_setting(&scope.root, value))
}

fn follow_global_root_jump(initial_root: PathBuf) -> PathBuf {
    let mut current = initial_root;
    let mut visited = Vec::<PathBuf>::new();

    loop {
        if visited.iter().any(|path| path == &current) {
            return current;
        }
        visited.push(current.clone());

        let next = load_scope_config(&current)
            .ok()
            .and_then(|config| config.settings.global_root)
            .map(|value| resolve_path_setting(&current, &value));

        match next {
            Some(path) if path != current => current = path,
            _ => return current,
        }
    }
}

fn resolve_path_setting(root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn prepare_remote_file(scope: &LoadedScope, remote: &RemoteFileConfig) {
    let destination = remote_destination(scope, remote);
    if !should_fetch_remote(&destination, remote.ttl.unwrap_or(-1)) {
        return;
    }

    let parent = match destination.parent() {
        Some(parent) => parent,
        None => return,
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }

    let response = match reqwest::blocking::get(&remote.url) {
        Ok(response) => response,
        Err(_) => return,
    };
    if !response.status().is_success() {
        return;
    }
    let bytes = match response.bytes() {
        Ok(bytes) => bytes,
        Err(_) => return,
    };
    let _ = fs::write(destination, &bytes);
}

fn remote_destination(scope: &LoadedScope, remote: &RemoteFileConfig) -> PathBuf {
    let base = remote
        .destination
        .as_deref()
        .map(|path| resolve_path_setting(&scope.root, path))
        .unwrap_or_else(|| scope.root.join("remote"));
    base.join(&remote.filename)
}

fn should_fetch_remote(path: &Path, ttl: i64) -> bool {
    if !path.exists() {
        return true;
    }
    if ttl < 0 {
        return false;
    }
    if ttl == 0 {
        return true;
    }

    let modified = match fs::metadata(path).and_then(|metadata| metadata.modified()) {
        Ok(modified) => modified,
        Err(source) if source.kind() == ErrorKind::NotFound => return true,
        Err(_) => return true,
    };
    let age = match SystemTime::now().duration_since(modified) {
        Ok(age) => age,
        Err(_) => Duration::from_secs(0),
    };

    age.as_secs() >= ttl as u64
}
