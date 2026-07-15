use std::fs;
use std::path::Path;

use crate::cli::InitScope;
use crate::config::{self, ScopeKind};
use crate::errors::{Result, ensure_absent};

pub fn scaffold(scope: InitScope, root: &Path) -> Result<()> {
    let kind = match scope {
        InitScope::Global => ScopeKind::Global,
        InitScope::Local => ScopeKind::Local,
    };

    config::validate_scope_root(root, kind)?;

    let config_path = root.join(config::CONFIG_FILE);
    ensure_absent(&config_path)?;

    fs::create_dir_all(root.join("context"))?;
    fs::create_dir_all(root.join("stores/decisions"))?;
    fs::create_dir_all(root.join("stores/memory"))?;
    fs::create_dir_all(root.join("stores/tickets"))?;

    fs::write(&config_path, config_template(scope))?;

    for (relative_path, contents) in context_templates(scope) {
        let path = root.join(relative_path);
        ensure_absent(&path)?;
        fs::write(path, contents)?;
    }

    Ok(())
}

fn config_template(scope: InitScope) -> &'static str {
    match scope {
        InitScope::Global => {
            r#"version = 1

[context]
include = [
  "context/agents.md",
  "context/preferences.md",
  "context/workflow.md",
]

[stores]
decisions = "stores/decisions"
memory = "stores/memory"
tickets = "stores/tickets"
"#
        }
        InitScope::Local => {
            r#"version = 1

[context]
include = [
  "context/project.md",
  "context/tools.md",
]

[stores]
decisions = "stores/decisions"
memory = "stores/memory"
tickets = "stores/tickets"
"#
        }
    }
}

fn context_templates(scope: InitScope) -> Vec<(&'static str, &'static str)> {
    match scope {
        InitScope::Global => vec![
            (
                "context/agents.md",
                "# Global agent context\n\nShared operating rules for all agents.\n",
            ),
            (
                "context/preferences.md",
                "# Global preferences\n\nResponse and workflow preferences that travel between projects.\n",
            ),
            (
                "context/workflow.md",
                "# Global workflow\n\nCross-project methodology, review expectations, and safety rules.\n",
            ),
        ],
        InitScope::Local => vec![
            (
                "context/project.md",
                "# Project context\n\nProject-specific architecture, conventions, and constraints.\n",
            ),
            (
                "context/tools.md",
                "# Project tools\n\nLanguages, package managers, linters, and runtime details for this repo.\n",
            ),
        ],
    }
}
