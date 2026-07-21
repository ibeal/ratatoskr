use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::errors::{RatatoskrError, Result};

pub const LOCAL_STATE_DIR: &str = ".rata";
pub const CONFIG_FILE: &str = "rata.toml";
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
    pub stores: BTreeMap<String, StoreConfig>,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StoreConfig {
    Legacy(String),
    Detailed(StoreDefinition),
}

impl StoreConfig {
    pub fn path(&self) -> &str {
        match self {
            Self::Legacy(path) => path,
            Self::Detailed(store) => &store.path,
        }
    }

    pub fn composition(&self) -> Option<StoreComposition> {
        match self {
            Self::Legacy(_) => None,
            Self::Detailed(store) => store.composition,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StoreDefinition {
    pub path: String,
    #[serde(default)]
    pub composition: Option<StoreComposition>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum StoreComposition {
    #[default]
    Replace,
    GlobalFirst,
    LocalFirst,
}

#[derive(Debug, Clone)]
pub struct LoadedScope {
    pub kind: ScopeKind,
    pub root: PathBuf,
    pub config: ScopeConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveSettings {
    pub allow_missing: bool,
    pub global_root: PathBuf,
    pub layers: Vec<SettingsLayer>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsLayer {
    pub scope_kind: String,
    pub scope_root: PathBuf,
    pub allow_missing: Option<bool>,
    pub global_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteFileStatus {
    pub name: String,
    pub scope_kind: String,
    pub scope_root: PathBuf,
    pub url: String,
    pub destination: PathBuf,
    pub ttl: i64,
    pub status: RemoteStatusKind,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteStatusKind {
    Present,
    Fetched,
    Refetched,
    Missing,
    FetchFailed,
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
    home_dir().join(".rata")
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
        .filter_map(|dir| dir.join(CONFIG_FILE).is_file().then_some(dir.to_path_buf()))
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
    if kind == ScopeKind::Local && root.join(CONFIG_FILE).exists() {
        return Err(RatatoskrError::InvalidRoot(format!(
            "local scope already contains `{CONFIG_FILE}`: {}",
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

pub fn resolve_effective_settings(
    global: Option<&LoadedScope>,
    locals: &[LoadedScope],
    global_root_override: Option<&Path>,
) -> EffectiveSettings {
    let mut allow_missing = true;
    let mut layers = Vec::new();

    if let Some(scope) = global {
        if let Some(value) = scope.config.settings.allow_missing {
            allow_missing = value;
        }
        layers.push(SettingsLayer {
            scope_kind: scope.kind.label().to_string(),
            scope_root: scope.root.clone(),
            allow_missing: scope.config.settings.allow_missing,
            global_root: resolve_scope_global_root(scope),
        });
    }

    for scope in locals {
        if let Some(value) = scope.config.settings.allow_missing {
            allow_missing = value;
        }
        layers.push(SettingsLayer {
            scope_kind: scope.kind.label().to_string(),
            scope_root: scope.root.clone(),
            allow_missing: scope.config.settings.allow_missing,
            global_root: resolve_scope_global_root(scope),
        });
    }

    EffectiveSettings {
        allow_missing,
        global_root: resolve_global_root(global_root_override, locals),
        layers,
    }
}

pub fn prepare_remote_files(scope: &LoadedScope) -> Vec<RemoteFileStatus> {
    scope
        .config
        .remote_files
        .iter()
        .map(|(name, remote)| prepare_remote_file(scope, name, remote))
        .collect()
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
    let path = expand_home_path(value);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn expand_home_path(value: &str) -> PathBuf {
    if value == "~" {
        return home_dir();
    }

    if let Some(suffix) = value.strip_prefix("~/") {
        return home_dir().join(suffix);
    }

    PathBuf::from(value)
}

fn prepare_remote_file(
    scope: &LoadedScope,
    name: &str,
    remote: &RemoteFileConfig,
) -> RemoteFileStatus {
    let destination = remote_destination(scope, remote);
    let ttl = remote.ttl.unwrap_or(-1);
    let exists_before = destination.exists();
    if !should_fetch_remote(&destination, ttl) {
        return RemoteFileStatus {
            name: name.to_string(),
            scope_kind: scope.kind.label().to_string(),
            scope_root: scope.root.clone(),
            url: remote.url.clone(),
            destination,
            ttl,
            status: if exists_before {
                RemoteStatusKind::Present
            } else {
                RemoteStatusKind::Missing
            },
            detail: None,
        };
    }

    let parent = match destination.parent() {
        Some(parent) => parent,
        None => {
            return RemoteFileStatus {
                name: name.to_string(),
                scope_kind: scope.kind.label().to_string(),
                scope_root: scope.root.clone(),
                url: remote.url.clone(),
                destination,
                ttl,
                status: RemoteStatusKind::FetchFailed,
                detail: Some("remote destination has no parent directory".to_string()),
            };
        }
    };
    if let Err(source) = fs::create_dir_all(parent) {
        return RemoteFileStatus {
            name: name.to_string(),
            scope_kind: scope.kind.label().to_string(),
            scope_root: scope.root.clone(),
            url: remote.url.clone(),
            destination,
            ttl,
            status: RemoteStatusKind::FetchFailed,
            detail: Some(source.to_string()),
        };
    }

    let response = match reqwest::blocking::get(&remote.url) {
        Ok(response) => response,
        Err(source) => {
            return RemoteFileStatus {
                name: name.to_string(),
                scope_kind: scope.kind.label().to_string(),
                scope_root: scope.root.clone(),
                url: remote.url.clone(),
                destination,
                ttl,
                status: RemoteStatusKind::FetchFailed,
                detail: Some(source.to_string()),
            };
        }
    };
    if !response.status().is_success() {
        return RemoteFileStatus {
            name: name.to_string(),
            scope_kind: scope.kind.label().to_string(),
            scope_root: scope.root.clone(),
            url: remote.url.clone(),
            destination,
            ttl,
            status: RemoteStatusKind::FetchFailed,
            detail: Some(format!("http {}", response.status())),
        };
    }
    let bytes = match response.bytes() {
        Ok(bytes) => bytes,
        Err(source) => {
            return RemoteFileStatus {
                name: name.to_string(),
                scope_kind: scope.kind.label().to_string(),
                scope_root: scope.root.clone(),
                url: remote.url.clone(),
                destination,
                ttl,
                status: RemoteStatusKind::FetchFailed,
                detail: Some(source.to_string()),
            };
        }
    };
    if let Err(source) = fs::write(&destination, &bytes) {
        let detail = RatatoskrError::WriteRemoteFile(destination.clone(), source).to_string();
        return RemoteFileStatus {
            name: name.to_string(),
            scope_kind: scope.kind.label().to_string(),
            scope_root: scope.root.clone(),
            url: remote.url.clone(),
            destination,
            ttl,
            status: RemoteStatusKind::FetchFailed,
            detail: Some(detail),
        };
    }

    RemoteFileStatus {
        name: name.to_string(),
        scope_kind: scope.kind.label().to_string(),
        scope_root: scope.root.clone(),
        url: remote.url.clone(),
        destination,
        ttl,
        status: if exists_before {
            RemoteStatusKind::Refetched
        } else {
            RemoteStatusKind::Fetched
        },
        detail: None,
    }
}

fn remote_destination(scope: &LoadedScope, remote: &RemoteFileConfig) -> PathBuf {
    let base = remote
        .destination
        .as_deref()
        .map(|path| resolve_path_setting(&scope.root, path))
        .unwrap_or_else(|| scope.root.join(LOCAL_STATE_DIR).join("remotes"));
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        default_global_root, discover_local_roots, expand_home_path, home_dir, load_local_scopes,
        resolve_global_root,
    };

    #[test]
    fn global_root_setting_expands_home_directory() {
        let root = temp_dir("config-global-root");
        let config_root = root.join("config-root");
        fs::create_dir_all(&config_root).unwrap();
        fs::write(
            config_root.join("rata.toml"),
            "version = 1\n\n[settings]\nglobal_root = \"~/dotfiles/agents/\"\n",
        )
        .unwrap();

        let locals = load_local_scopes(&root).unwrap();
        let resolved = resolve_global_root(Some(&config_root), &locals);

        assert_eq!(resolved, home_dir().join("dotfiles/agents/"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn expand_home_path_handles_tilde_prefix() {
        assert_eq!(expand_home_path("~"), home_dir());
        assert_eq!(
            expand_home_path("~/dotfiles/agents/"),
            home_dir().join("dotfiles/agents/")
        );
    }

    #[test]
    fn local_scope_discovery_uses_rata_toml_in_ancestor_directories() {
        let root = temp_dir("discover-local-roots");
        let workspace = root.join("workspace");
        let project = workspace.join("project");
        let nested = project.join("src");

        fs::create_dir_all(&nested).unwrap();
        fs::write(workspace.join("rata.toml"), "version = 1\n").unwrap();
        fs::write(project.join("rata.toml"), "version = 1\n").unwrap();
        fs::create_dir_all(project.join(".rata")).unwrap();

        assert_eq!(
            discover_local_roots(&nested),
            vec![workspace.clone(), project.clone()]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn default_global_root_uses_hidden_home_directory() {
        assert_eq!(default_global_root(), home_dir().join(".rata"));
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
}
