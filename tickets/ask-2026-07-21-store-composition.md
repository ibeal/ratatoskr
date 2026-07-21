# Store composition

- ID: ask-2026-07-21-store-composition
- Source: plain ask
- Status: in_progress
- Phase: build
- Created: 2026-07-21
- Updated: 2026-07-21

## Spec

### Goal

Allow each named store to choose whether resolution uses only its most-specific path or all scoped
paths in a deterministic order.

### Acceptance criteria

1. `rata.toml` accepts a per-store `composition` of `replace`, `global-first`, or `local-first`.
2. Existing string-valued store declarations remain valid; `replace` applies when no scope sets a
   composition.
3. Store resolution returns ordered paths for every resolved store.
4. The nearest declaration that specifies composition controls matching global and local store
   paths; path-only local declarations inherit an explicit outer policy.
5. Schema, docs, and tests describe and verify the behavior.
6. `rata doctor stores` shows every store layer and its effective composition.
7. `rata doctor settings` shows effective settings and every settings layer, while base `rata doctor`
   remains limited to health, layers, and errors.

### Non-goals

- Configuring collision handling for store contents.
- Adding a separate write-target setting.

## Intake

### Recommendation

Go.

### Reasoning

The three modes express the required visibility and ranking without embedding store-specific
retrieval or collision behavior in Ratatoskr.

## Journal

### 2026-07-21

- Defined `replace` as the backwards-compatible default for legacy string-valued store entries.
- Defined `global-first` and `local-first` as ordered views across every matching scope; the
  nearest store declaration chooses the composition.
- Implemented legacy string and inline-table store declarations, layered resolution, schema and
  README updates, and coverage for all three composition modes.
- Verified with `cargo fmt --check`, `cargo test`, `cargo check`, schema validation, and a live
  `rata resolve stores --format json` run against the existing global context.
- Refined composition inheritance: only explicit composition values override an outer policy, and
  `replace` is applied only when no active scope specifies one.
- Kept composition internal to resolution; `rata resolve` output shows only ordered store paths.
- Added `rata doctor stores` for store layers and effective composition, and `rata doctor settings`
  for effective settings plus their source layers; reduced base `rata doctor` to health, layers,
  and errors.
