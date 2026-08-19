# ask-2026-08-19-ref-space-show — Unified ref space + `rata show`

| | |
|---|---|
| **Source** | plain ask |
| **Spec** | authored here |
| **Phase** | intake |
| **PR** | — |
| **Project(s)** | ratatoskr |
| **Created** | 2026-08-19 |
| **Updated** | 2026-08-19 |
| **Depends on** | `ask-2026-08-19-node-signatures-outline` |

---

## Spec — acceptance criteria

**Context.** The outline slice makes nodes *listable*. This one makes them *addressable* — one ref
syntax that resolves a store file, a heading inside a context file, or a heading inside a store file,
so an agent can step from a bird's-eye listing into exactly one section without loading its file.
This is the go-to-definition half of the codebase analogy.

**Refined (after intake):**

- Define one ref syntax covering both granularities:
  - `memory:containerized-agents` — a store node
  - `AGENTS.md#Safety` — a heading in a context file
  - `workflow/sdlc.md#Phases/PR summaries` — a nested heading, path-addressed
- Headings become nodes with the same signature/body model as files: a heading's **body** is the
  prose under it *minus its descendants*, and its signature resolves via the same ladder.
- Add `rata show <ref> [--depth N]`:
  - depth 0 (default) — the node's body plus the **signatures** of its children, not their bodies
  - depth N — descend N levels, bodies included
- Extend `rata outline` to accept a file ref and render its heading tree, so outline and show operate
  over one model at both granularities.
- Addressing is **stable against heading edits** where it matters: support explicit `{#slug}` anchors
  and honour them in preference to heading text; fall back to heading-path for everything else.
- An unresolvable ref fails with the closest candidates listed, not a bare error.

**Explicitly out of scope:**

- Reverse edges (`callers`) and graph export — next slice.
- Any TUI or LSP. Ruled out as overkill; the verbs are the interface.

**Verification:** `rata show workflow/sdlc.md#PR-summaries` returns that section alone; `rata show
memory:containerized-agents` returns the file body; `rata outline AGENTS.md` returns its heading tree
with signatures.

---

## Journal

### Intake — "should we build this?"

- **Alignment:** completes the pack/only split. `pack` gives the eager root set, `outline` gives the
  map, `show` is the fetch. Today `only` is file-granular and can't address a section, so an agent
  wanting one part of `sdlc.md` loads all 324 lines.
- **Alternatives considered:** file-granular only (simpler, but `sdlc.md` is the single biggest
  context file and is exactly where sub-file addressing pays); Obsidian-style block refs (`^id` per
  block — finer than needed and requires authoring per block).
- **AC sanity:** heading-path addressing is fragile under heading renames, hence the `{#slug}`
  escape hatch for anything cross-referenced. Accepting that fragility for incidental refs keeps
  authoring cost at zero for the common case.
- **Recommendation:** go.
- **Human's decision:** pending.

### Decisions carried in from design

- **Two granularities, one model.** Files and headings are both nodes with a signature and a body;
  inter-file (index → leaves) and intra-file (heading → subheading) are the same navigation at
  different scales.
- **Same verbs for human and agent.** No separate human UI. If Ian walks the graph with exactly what
  the agent walks it with, he can always reproduce what the agent saw, and every ergonomic
  improvement is a context improvement. A bespoke TUI decouples the two and they drift.

### Build log

- 2026-08-19: Spec authored from design discussion. Not started.

### Open questions

- Does `show` on a store node need `--depth` at all, or is depth only meaningful for headings?

### Checkpoints (memory boundaries)

- **PR-up:** —
- **Merge:** —
