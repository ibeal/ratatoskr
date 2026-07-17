# ratatoskr

`ratatoskr` is a filesystem-first CLI for managing portable AI-agent context.

The project name is `ratatoskr`. The command is `rata`.

The name comes from Ratatoskr (`RAH-tah-toss-ker`), the squirrel in Norse mythology who runs along Yggdrasil, the world tree, carrying messages and insults between the eagle in its branches and Nidhogg at its roots. 

## Install

If you use Nix, you can run or install `rata` directly from this repo:

```text
nix run .#
nix build .#
nix profile install .#
```

From GitHub:

```text
nix run github:ibeal/ratatoskr
nix profile install github:ibeal/ratatoskr
```

For local development:

```text
nix develop
```

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

1. Global root: `~/.rata`
2. Local scopes: every ancestor directory containing `rata.toml`, found by walking upward from the current directory

Global root precedence is:

1. `--global-root <path>`
2. `RATA_ROOT`
3. nearest local `rata.toml` with `[settings].global_root`
4. `~/.rata/rata.toml` with `[settings].global_root`
5. `~/.rata`

Each root contains:

```text
rata.toml
  .rata/context/
  context/
  stores/
.rata/
  remotes/
```

The config file declares:

- ordered context file includes
- additive profiles that include additional context files
- remote files to fetch into a local cache
- settings like `allow_missing` and `global_root`
- named datastore directories

Settings compose in scope order too. `allow_missing` defaults to `true`, and later scopes can
override it. `global_root` can redirect which global root is used for a subtree or for the default
global root itself. `rata resolve` exposes the effective result plus a `settings_layers` trace so
you can see global and local values and where later scopes overrode earlier ones.

Remote files live in a separate `[remote_files]` section. They are fetched on a best-effort basis
before resolution. Fetch failures never raise on their own. If you reference a cached remote file
from `[context].include` and want that absence to fail, set `allow_missing = false`.
`rata doctor` reports remote cache status and missing context files explicitly.

Remote defaults:

- `destination` defaults to `.rata/remotes/` next to the defining `rata.toml`
- `ttl` defaults to `-1`, which means never refetch if the cached file already exists

`rata init` also writes a Taplo schema directive at the top of generated `rata.toml` files so
editors can validate against the schema published in this repo at
`schema/rata.schema.json`.

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
rata doctor
rata doctor --format json
rata pack
rata only profile build
rata only scope local
rata only file agents.md
rata pack --format json
rata docs agent
```

You can also point the default global root somewhere else using `~/.rata/rata.toml`:

```toml
[settings]
global_root = "/Users/ian/src/agent-context"
```

And a local scope can override the global root for everything inside that subtree:

```toml
[settings]
global_root = "../../shared/work-context"
```

A nested layout like this is supported:

```text
~/src/
  ap/
    rata.toml
  ap/service-a/
    rata.toml
```

Running `rata resolve` inside `service-a` will compose:

1. global scope
2. `~/src/ap`
3. `~/src/ap/service-a`

## Example layout

`rata init global` creates the global root at `~/.rata/`.

`rata init local` creates only two root-level entries in the current directory:

- `rata.toml`
- `.rata/`

```text
~/.rata/
  rata.toml
  .rata/
    context/
      agents.md
      preferences.md
    remotes/
  stores/
    memory/

<repo>/
  rata.toml
  .rata/
    context/
      project.md
      tools.md
      standards.md
      review-checklist.md
    remotes/
    stores/
      decisions/
      memory/
      tickets/
```

## Example config

```toml
#:schema https://raw.githubusercontent.com/ibeal/ratatoskr/main/schema/rata.schema.json

version = 1

[context]
include = [
  ".rata/context/project.md",
  ".rata/context/tools.md",
  ".rata/remotes/architecture.md",
]

[settings]
allow_missing = true

[remote_files.architecture]
url = "https://example.com/architecture.md"
filename = "architecture.md"
ttl = -1

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
```

## Next steps

- add `show stack` and `show context`
- add store helpers for recency-based reads and explicit named roots
