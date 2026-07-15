# ratatoskr

`ratatoskr` is a filesystem-first CLI for managing portable AI-agent context.

The project name is `ratatoskr`. The command is `rata`.

## Goal

Provide a small, agent-agnostic layer for:

- global context like `agents.md`, `preferences.md`, and shared workflow docs
- global datastores like `decisions/`, `memory/`, and `tickets/`
- local project context for architecture, tools, and repo-specific conventions
- local project datastores that live beside the code they describe

The first version is intentionally narrow. It discovers roots, scaffolds directory layouts, and
resolves the active context stack. It does not yet index content, search stores, or integrate with
specific agent products.

## Model

Ratatoskr resolves context in layers:

1. Global root: `~/.config/ratatoskr`
2. Local root: the nearest `.ratatoskr/` found by walking upward from the current directory

Each root contains:

```text
ratatoskr.toml
context/
stores/
```

The config file declares:

- ordered context file includes
- named datastore directories

Local scope overrides global scope by store name because project-specific state should win over
portable defaults.

## Current commands

```text
rata init global
rata init local
rata resolve
rata resolve --format json
```

## Example layout

```text
~/.config/ratatoskr/
  ratatoskr.toml
  context/
    agents.md
    preferences.md
    workflow.md
  stores/
    decisions/
    memory/
    tickets/

<repo>/.ratatoskr/
  ratatoskr.toml
  context/
    project.md
    tools.md
  stores/
    decisions/
    memory/
    tickets/
```

## Example config

```toml
version = 1

[context]
include = [
  "context/project.md",
  "context/tools.md",
]

[stores]
decisions = "stores/decisions"
memory = "stores/memory"
tickets = "stores/tickets"
```

## Next steps

- add `show stack` and `show context`
- add `pack` for bundling resolved context into markdown or JSON payloads
- add include/exclude profiles for different agent workflows
- add store helpers for recency-based reads and explicit named roots
