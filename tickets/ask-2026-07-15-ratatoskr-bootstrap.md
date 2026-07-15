# Ratatoskr bootstrap

- ID: `ask-2026-07-15-ratatoskr-bootstrap`
- Source: plain ask in chat
- Status: in_progress
- Phase: build
- Created: 2026-07-15
- Updated: 2026-07-15

## Spec

### Goal

Create the first commit for `ratatoskr`, a CLI skeleton for context management across AI agents.

### Acceptance criteria

1. The repo contains a minimal, runnable CLI named `ratatoskr` with command name `rata`.
2. The CLI models both global and local configuration roots for:
   - context files
   - named datastore directories
3. The repository includes an opinionated directory layout and config format for both global and
   local scopes.
4. The code implements discovery and resolution primitives only; it does not yet implement
   agent-specific integrations, indexing, search, or persistence beyond files and folders.
5. The repo includes a README describing the vision, terminology, initial architecture, and next
   steps.
6. The repo includes enough scaffolding to support future commands such as `init`, `resolve`, and
   `pack`, even if only one or two commands are wired in initially.
7. The change is committed as the initial project bootstrap.

### Non-goals

- No embeddings, vector DB, or full-text indexing.
- No external service integration.
- No automatic mutation of agent config files.
- No project-specific data model beyond file-backed config and directories.

## Intake

### Recommendation

Go.

### Reasoning

- The requested scope is intentionally narrow and suitable for a first bootstrap commit.
- A file-and-merge model keeps the design agent-agnostic and easy to evolve.
- The primary open decision is implementation language, not product direction.

### Proposed implementation choice

- Start in Rust.
- Use a single-binary CLI with `clap` for argument parsing and `toml` + `serde` for config.

## Journal

### 2026-07-15

- Created the initial spec from the plain ask.
- Repo was empty at intake time.
- Confirmed the acceptance criteria with the human and moved into build.
- Scaffolded the Rust crate with `rata init` and `rata resolve` as the first commands.
- Added file-backed global/local root config, upward local discovery, and merged manifest output.
- Verified with `cargo fmt`, `cargo check`, `rata resolve --format json`, and a scaffolded local-root
  `rata init` + `rata resolve` flow.
- Confirmed the intended GitHub visibility is `public` before the first push.
- Extended the config model with additive profiles so conditional context can be activated on demand.
- Added `--profile` selection to `rata resolve`, exposed available profile names, and verified
  global + local profile composition for the same selected profile.
- Added custom global-root resolution with command-line and environment overrides so a cloned
  context repo can act as the active global scope without living under `~/.config/rata`.
- Added a machine-local pointer file at `~/.config/.rata/.rata.toml` so the default
  discovery path can redirect to a cloned global context repo.
- Expanded local discovery from a single nearest scope to a full ancestor scope stack so nested
  layouts like `global -> ap -> project` compose automatically.
- Renamed runtime-facing filesystem conventions from `ratatoskr` to `rata`, including `.rata`
  local roots, `.rata.toml` manifests, `~/.config/rata`, `~/.config/.rata/.rata.toml`, and
  `RATA_ROOT`.
- Added `rata pack`, which reads the resolved context file list and emits a deterministic bundle in
  text or JSON using the same scope/profile selection rules as `rata resolve`.
- Added agent-focused usage docs as a versioned repo document plus `rata docs agent` so the CLI can
  serve workflow guidance directly.
- Refined the agent flow so startup uses `rata pack`, profiles are opt-in follow-up packs, and
  `rata resolve stores` exposes durable store locations without mixing them into prompt context.
