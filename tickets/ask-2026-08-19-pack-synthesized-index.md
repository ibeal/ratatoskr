# ask-2026-08-19-pack-synthesized-index — `pack` synthesizes store indexes

| | |
|---|---|
| **Source** | plain ask |
| **Spec** | authored here |
| **Phase** | build |
| **PR** | — (local stacked commit) |
| **Project(s)** | ratatoskr, dotfiles/agents |
| **Created** | 2026-08-19 |
| **Updated** | 2026-08-19 |
| **Depends on** | `ask-2026-08-19-node-signatures-outline` |

---

## Spec — acceptance criteria

**Context.** `agents/rata.toml` lists `memory/MEMORY.md` in `[context].include`, and that file is a
hand-maintained list of pointers to the other files in the same store. Every new memory requires a
second edit to stay discoverable, and nothing enforces it. The signature ladder makes that list
derivable, so it should stop existing on disk.

**Refined (after intake):**

- `[context].include` accepts a **store ref** (e.g. `memory:`) alongside file paths.
- When `pack` encounters a store ref, it renders that store's computed outline inline, at the same
  position and with the same section framing a file include gets today.
- The rendered block is visibly marked as generated, so a reader knows it has no source file.
- `pack` output stays **deterministic** — stable ordering (filename, unless a later slice adds an
  ordering key), so two runs over an unchanged store are byte-identical.
- Retire `agents/memory/MEMORY.md` as a hand-maintained index. Its non-pointer preamble, if worth
  keeping, moves into the store's own file or the rendered block's header — it is not silently lost.
- Drop the `@memory/MEMORY.md` transclusion from `AGENTS.md`. Agents reaching `AGENTS.md` only
  through `@`-imports will not see the memory index until they run `rata pack`; this is accepted,
  not an oversight (see Intake).
- `rata doctor` warns when a store contains a file that looks like a hand-maintained index of its
  siblings, so this doesn't quietly grow back.

**Explicitly out of scope:**

- Changing which stores are eager. This slice changes *how* an already-eager index is produced, not
  *what* gets packed.
- The marker-block / write-to-disk variant. Chosen against; see Decisions. Revisit only if a non-rata
  consumer needs the index as a real file.

**Verification:** `rata pack` before and after produces equivalent memory-index content; deleting
`memory/MEMORY.md` changes nothing in the pack output; adding a memory file appears in the next
`pack` with no other edit.

---

## Journal

### Intake — "should we build this?"

- **Alignment:** directly serves the "write it down, don't maintain it twice" discipline in
  `AGENTS.md`. The index is derived data and rata already owns derivation.
- **Alternatives considered:** (a) leave `MEMORY.md` hand-written — status quo, drifts;
  (b) generate into a marker block inside the real file (`<!-- rata:index memory -->`), keeping
  curation and killing staleness but leaving a file that can be edited out of sync between runs;
  (c) synthesize at pack time with no file at all. Chose (c); (b) stays available as the escape
  hatch for external consumers.
- **AC sanity:** the only current consumer of `memory/MEMORY.md` is `pack` itself (via
  `[context].include`) and the `@memory/MEMORY.md` transclusion in `AGENTS.md`. The latter needs
  updating in the same change or the file can't be removed — this slice spans both repos.
- **Risk (resolved):** `AGENTS.md` is `@`-imported directly by Claude Code and by the generated Codex
  shim, neither of which runs rata. Removing the `@memory/MEMORY.md` line means those paths no longer
  carry the memory index on their own. **Accepted** — see the human's decision.
- **Recommendation:** go.
- **Human's decision:** 2026-08-19 — accept the gap. `AGENTS.md` instructs every agent to run
  `rata pack` at session start, so the `@`-import path is a duplicate of what `pack` emits rather
  than the sole source. The exposure is the window before the first `rata pack` call, which is
  acceptable. Do **not** add a generated-on-write `MEMORY.md` to close it.

### Build log

- 2026-08-19: Spec authored from design discussion. Not started.
- 2026-08-19: **Scope cut by the human: ratatoskr only.** The rata-side capability is built in full;
  the `~/dotfiles/agents` migration (retiring `memory/MEMORY.md`, dropping its `@`-import from
  `AGENTS.md`, switching `rata.toml` to `memory:`) is Ian's to apply. See *Not done* below.
- 2026-08-19: Built the rata side.
  - `[context].include` entries now resolve to a `ContextTarget` — either a `File { path }` or a
    `StoreIndex { store }`. A store ref is a bare store name plus a trailing colon (`memory:`); the
    colon is what makes it unambiguous against a relative path. `ResolvedContextEntry.path` became
    `path()` returning `Option<&Path>`, since a synthesized index has no file.
  - `pack` renders a store ref as `## Store Index: <store>:` with a `generated:` line stating that
    rata computed it from a directory scan and no source file exists — same section framing as a
    file include, but impossible to mistake for something editable. Ordering is by ref, so an
    unchanged store packs byte-identically.
  - An unknown store ref is treated exactly like a missing file: filtered under
    `allow_missing = true`, reported by `doctor`, fatal when `allow_missing = false`. Reusing the
    existing semantics beat inventing a second absence rule.
  - `doctor` gained a **warnings** channel (separate from errors, so it does not affect `healthy`
    or the exit code) with a `hand_maintained_index` warning.
- 2026-08-19: The index-detection heuristic needed two passes. Counting sibling links alone either
  missed the real `memory/MEMORY.md` (a 2-file store, so any absolute threshold is wrong) or fired
  on ordinary prose that cites one sibling. What actually separates the two is **shape**: an index
  *is* a list of pointers. It now requires the file to have ≥2 list items, at least half of which
  link a sibling, plus links to at least half the store. Verified against the real five stores: it
  flags `memory/MEMORY.md` and nothing else.

### Not done (deliberately, and why)

Everything below is a `~/dotfiles/agents` change, excluded by the 2026-08-19 ratatoskr-only
decision. The rata side is complete, so each is now a one-line edit:

- Switch `rata.toml`'s `[context].include` from `"memory/MEMORY.md"` to `"memory:"`.
- Delete `memory/MEMORY.md`, keeping its non-pointer preamble (the "keep lean, one line per entry,
  date anything time-bound" guidance) somewhere — it is *not* derivable and would otherwise be lost.
  It is guidance about how to write memories, so `context/rata.md` is the natural home.
- Drop `@memory/MEMORY.md` from `AGENTS.md`.
- Note that `MEMORY.md` also points at `nix.md` at the dotfiles repo root, which is **outside** the
  memory store. A store ref will not surface it; that pointer needs a real home.

`rata doctor` already warns about `memory/MEMORY.md` today, so the reminder is in the tool rather
than only in this file.

### Open questions

- None blocking. The `@`-import question is settled (see Intake): the gap is accepted because
  `rata pack` runs at session start for every agent.

### Checkpoints (memory boundaries)

- **PR-up:** —
- **Merge:** —
