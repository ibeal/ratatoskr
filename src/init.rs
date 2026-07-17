use std::fs;
use std::path::Path;

use crate::cli::InitScope;
use crate::config::{self, ScopeKind};
use crate::errors::{Result, ensure_absent};

const SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/ibeal/ratatoskr/main/schema/rata.schema.json";

pub fn scaffold(scope: InitScope, root: &Path) -> Result<()> {
    let kind = match scope {
        InitScope::Global => ScopeKind::Global,
        InitScope::Local => ScopeKind::Local,
    };

    config::validate_scope_root(root, kind)?;

    let config_path = root.join(config::CONFIG_FILE);
    ensure_absent(&config_path)?;

    fs::create_dir_all(root.join(config::LOCAL_STATE_DIR).join("context"))?;
    fs::create_dir_all(root.join(config::LOCAL_STATE_DIR).join("remotes"))?;
    if matches!(scope, InitScope::Local) {
        fs::create_dir_all(root.join(config::LOCAL_STATE_DIR).join("stores/decisions"))?;
        fs::create_dir_all(root.join(config::LOCAL_STATE_DIR).join("stores/memory"))?;
        fs::create_dir_all(root.join(config::LOCAL_STATE_DIR).join("stores/tickets"))?;
    } else {
        fs::create_dir_all(root.join("stores/memory"))?;
    }

    fs::write(&config_path, config_template(scope))?;

    for (relative_path, contents) in context_templates(scope) {
        let path = root.join(relative_path);
        ensure_absent(&path)?;
        fs::write(path, contents)?;
    }

    Ok(())
}

fn config_template(scope: InitScope) -> String {
    match scope {
        InitScope::Global => format!(
            r#"
#:schema {SCHEMA_URL}

version = 1

[context]
include = [
  ".rata/context/agents.md",
  ".rata/context/preferences.md",
]

[settings]
allow_missing = true

[stores]
memory = "stores/memory"
"#
        )
        .trim_start_matches('\n')
        .to_string(),
        InitScope::Local => format!(
            r#"
#:schema {SCHEMA_URL}

version = 1

[context]
include = [
  ".rata/context/project.md",
  ".rata/context/tools.md",
]

[settings]
allow_missing = true

[profiles.build]
description = "Project-specific coding context"
include = [".rata/context/standards.md"]

[profiles.review]
description = "Project-specific review guidance"
include = [".rata/context/review-checklist.md"]

[stores]
decisions = ".rata/stores/decisions"
memory = ".rata/stores/memory"
tickets = ".rata/stores/tickets"
"#
        )
        .trim_start_matches('\n')
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::cli::InitScope;

    use super::{SCHEMA_URL, config_template, scaffold};

    #[test]
    fn global_template_starts_with_schema_directive() {
        let template = config_template(InitScope::Global);
        assert!(template.starts_with(&format!("#:schema {SCHEMA_URL}\n\n")));
    }

    #[test]
    fn local_template_starts_with_schema_directive() {
        let template = config_template(InitScope::Local);
        assert!(template.starts_with(&format!("#:schema {SCHEMA_URL}\n\n")));
    }

    #[test]
    fn local_template_uses_local_state_store_paths() {
        let template = config_template(InitScope::Local);
        assert!(template.contains("decisions = \".rata/stores/decisions\""));
        assert!(template.contains("memory = \".rata/stores/memory\""));
        assert!(template.contains("tickets = \".rata/stores/tickets\""));
    }

    #[test]
    fn local_scaffold_only_creates_rata_toml_and_dot_rata_at_root() {
        let root = temp_dir("init-local-layout");

        scaffold(InitScope::Local, &root).unwrap();

        assert!(root.join("rata.toml").is_file());
        assert!(root.join(".rata").is_dir());
        assert!(root.join(".rata/context").is_dir());
        assert!(root.join(".rata/remotes").is_dir());
        assert!(root.join(".rata/stores/decisions").is_dir());
        assert!(root.join(".rata/stores/memory").is_dir());
        assert!(root.join(".rata/stores/tickets").is_dir());
        assert!(!root.join("context").exists());
        assert!(!root.join("stores").exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn global_scaffold_creates_global_root_contents_under_target_root() {
        let root = temp_dir("init-global-layout");

        scaffold(InitScope::Global, &root).unwrap();

        assert!(root.join("rata.toml").is_file());
        assert!(root.join(".rata").is_dir());
        assert!(root.join(".rata/context").is_dir());
        assert!(root.join(".rata/remotes").is_dir());
        assert!(root.join("stores/memory").is_dir());
        assert!(!root.join("stores/decisions").exists());
        assert!(!root.join("stores/tickets").exists());

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
}

fn context_templates(scope: InitScope) -> Vec<(&'static str, &'static str)> {
    match scope {
        InitScope::Global => vec![
            (
                ".rata/context/agents.md",
                "# Global agent context\n\nShared operating rules for all agents.\n",
            ),
            (
                ".rata/context/preferences.md",
                "# Global preferences\n\nResponse and workflow preferences that travel between projects.\n",
            ),
        ],
        InitScope::Local => vec![
            (
                ".rata/context/project.md",
                "# Project context\n\nProject-specific architecture, conventions, and constraints.\n",
            ),
            (
                ".rata/context/tools.md",
                "# Project tools\n\nLanguages, package managers, linters, and runtime details for this repo.\n",
            ),
            (
                ".rata/context/standards.md",
                "# Project standards\n\nCoding conventions and implementation guidance specific to this repo.\n",
            ),
            (
                ".rata/context/review-checklist.md",
                "# Project review checklist\n\nProject-specific checks to apply during code review.\n",
            ),
        ],
    }
}
