# ask-2026-08-19-ref-space-show — Unified ref space + `rata show`

| | |
|---|---|
| **Source** | plain ask |
| **Spec** | authored here |
| **Phase** | build |
| **PR** | — (local stacked commit) |
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
- **Human's decision:** 2026-08-19 — go, as part of the series; stacked local commit, no PR.

### Decisions carried in from design

- **Two granularities, one model.** Files and headings are both nodes with a signature and a body;
  inter-file (index → leaves) and intra-file (heading → subheading) are the same navigation at
  different scales.
- **Same verbs for human and agent.** No separate human UI. If Ian walks the graph with exactly what
  the agent walks it with, he can always reproduce what the agent saw, and every ergonomic
  improvement is a context improvement. A bespoke TUI decouples the two and they drift.

### Build log

- 2026-08-19: Spec authored from design discussion. Not started.
- 2026-08-20: Built. Three new modules:
  - `src/headings.rs` — parses a file body into a heading forest. A heading's body is the prose
    under it minus its descendants; its signature uses the same ladder, minus the two rungs that
    cannot apply (no frontmatter, no filename), so first-sentence then title. Fences are tracked so
    a `#` in a shell block is not a heading, and levels may skip (an H1 followed by an H3 nests
    correctly).
  - `src/refs.rs` — one parser and one resolver for the whole ref space. `Ref::parse` splits
    `store:node#heading/path`; `RefSpace` indexes context files (by scope-relative path *and*
    absolute path) plus every store node.
  - `src/show.rs` — the `rata show` report.
- 2026-08-20: **The H1 collapse was the design call that made the syntax usable.** Rendering
  addresses naively produced
  `AGENTS.md#agents-md-personal-operating-manual/safety` — the H1 slug in every ref, adding nothing.
  A lone top-level heading is now treated as the *file's title* rather than a section inside it: its
  prose becomes the file node's own body and its children become the file's children. That yields
  `AGENTS.md#Safety` (the syntax the AC actually specifies) and it also makes `show <file>` return
  the intro prose plus the section signatures, which is the useful thing. Sibling H1s are still real
  sections and are not collapsed. This replaced an earlier skip-the-title special case in the
  resolver, which was the same idea in the wrong place.
- 2026-08-20: `outline` takes one positional that is either a store name or a file ref; a store wins
  when the name matches one, so `rata outline memory` keeps its old meaning.
- 2026-08-20: Two resolution refinements found by running it:
  - A shorthand (`sdlc.md`) matched both the relative and absolute address of the *same* file and
    was rejected as ambiguous. Ambiguity is now judged on resolved paths, not address strings.
  - Candidate lists preferred absolute paths. They now prefer the scope-relative form, which is how
    refs are meant to be written.

### Open questions

- ~~Does `show` on a store node need `--depth` at all, or is depth only meaningful for headings?~~
  **Depth is uniform, and it is meaningful for files.** A file's children are its top-level
  headings, so one rule covers both granularities and there is nothing extra to remember. The AC's
  "`show memory:containerized-agents` returns the file body" still holds because that file has no
  subheadings — with no descendants to exclude, a file's own body *is* the whole file. Special-casing
  files would have made depth mean two different things.

### Checkpoints (memory boundaries)

- **PR-up:** —
- **Merge:** —
