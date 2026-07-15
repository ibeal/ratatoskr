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

1. Global root: `~/.config/rata`
2. Local scopes: every ancestor `.rata/` found by walking upward from the current directory

Global root precedence is:

1. `--global-root <path>`
2. `RATA_ROOT`
3. `~/.config/.rata/.rata.toml` with `root = "/path/to/context-root"`
4. `~/.config/rata`

Each root contains:

```text
.rata.toml
context/
stores/
```

The config file declares:

- ordered context file includes
- additive profiles that include additional context files
- named datastore directories

Scopes compose in order: global first, then outer local scopes, then inner local scopes. Store names
override by last writer, so more specific scopes win.

Profiles compose across scopes too. If global, `ap/`, and project scopes all define `build`, then
`rata resolve --profile build` activates all of them in scope order.

## Current commands

```text
rata init global
rata init local
rata resolve summary
rata resolve stores
rata resolve --global-root ~/src/agent-context
rata resolve stores --format json
rata resolve --format json
rata pack
rata pack --profile build
rata pack --format json
rata docs agent
```

You can also keep a machine-local pointer file at `~/.config/.rata/.rata.toml`:

```toml
root = "/Users/ian/src/agent-context"
```

That lets you keep your portable global context in a cloned repo without requiring it to live under
`~/.config/rata`.

A nested layout like this is supported:

```text
~/src/
  ap/
    .rata/
  ap/service-a/
    .rata/
```

Running `rata resolve` inside `service-a` will compose:

1. global scope
2. `~/src/ap/.rata`
3. `~/src/ap/service-a/.rata`

## Example layout

```text
~/.config/rata/
  .rata.toml
  context/
    agents.md
    preferences.md
    sdlc.md
  stores/
    decisions/
    memory/
    tickets/

<repo>/.rata/
  .rata.toml
  context/
    project.md
    tools.md
    standards.md
    review-checklist.md
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

[profiles.build]
description = "Project-specific coding context"
include = ["context/standards.md"]

[profiles.review]
description = "Project-specific review guidance"
include = ["context/review-checklist.md"]

[stores]
decisions = "stores/decisions"
memory = "stores/memory"
tickets = "stores/tickets"
```

## Next steps

- add `show stack` and `show context`
- add store helpers for recency-based reads and explicit named roots
