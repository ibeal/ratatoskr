# Version output

- ID: ask-2026-07-21-version-output
- Source: plain ask
- Status: in_progress
- Phase: build
- Created: 2026-07-21
- Updated: 2026-07-21

## Spec

### Goal

Expose the Ratatoskr package version and Git revision from the CLI.

### Acceptance criteria

1. `rata --version` and `rata -v` print the package version and Git SHA.
2. The Git SHA is embedded at build time.
3. Builds without Git metadata still compile and print an explicit fallback value.

## Intake

### Recommendation

Go.

### Reasoning

Version output makes installed binaries traceable without adding a runtime Git dependency.

## Journal

### 2026-07-21

- Added a build-time Git SHA environment value with an `unknown` fallback and configured Clap to
  expose it through the requested lowercase `-v` and `--version` flags.
- Verified both flags print the package version and embedded full SHA, alongside `cargo fmt`,
  `cargo test`, `cargo check`, help output, and diff checks.
- Corrected the custom Clap version flag so its parser-only backing field is explicitly optional;
  this must not be treated as context configuration or shown by `rata doctor`.
