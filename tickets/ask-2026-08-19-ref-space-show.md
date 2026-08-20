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

### Fresh-eyes review (2026-08-20)

Subagent, fed only the three diffs + the AC + the checklist. Findings applied:

**Blocking:**
- **A lone top-level heading was collapsed even when it was not an H1, deleting the section.** The
  guard read `tree.len() == 1 && tree[0].children.iter().all(|c| c.level > 1)` — and `nest` can
  never produce a child at or above its parent's level, so the second clause was *vacuously true*
  and the guard was really just `tree.len() == 1`. A file whose only top-level heading is an H2
  had that heading silently absorbed and its children re-rooted, making every ref for that file
  wrong. Now `tree[0].level == 1`. Only an H1 titles a file.

**Should-fix, applied:**
- **Indented code blocks parsed as headings.** Indentation was trimmed before the `#` check, so a
  4-space-indented `# comment` inside a list item became a heading and re-parented everything after
  it. ATX indent is now capped at three columns, counting a tab as four.
- **Duplicate and empty slugs produced refs that `outline` printed but `show` could not resolve.**
  Two `## Dup` headings yielded the same ref; `## ---` slugified to the empty string and printed as
  `file.md#`. Slugs are now made unique among siblings (`dup`, `dup-2`) and non-empty (`section`)
  before addresses are built. Verified over the real corpus: all 33 heading refs `outline` prints
  round-trip through `show`.
- **An explicit `{#a/b}` anchor minted an unresolvable ref** — `/` is the heading-path separator, so
  the address could never be walked, and the failure even suggested the ref that had just failed.
  An anchor containing `/` or `#` is now refused and the title is slugified instead, matching how
  `node_ref` already rejects separators in filenames.
- Setext headings (a title over `===` or `---`) are now recognized; a trailing run of `#` is treated
  as a closing sequence rather than part of the title; a tab after the hashes opens a heading; and
  fence tracking remembers *which* marker opened, so a `~~~` line inside a ``` block no longer ends
  it early.
- **Two files sharing a scope-relative address silently last-wins.** A local `AGENTS.md` shadowing
  the global one made that address resolve to whichever was inserted last, with the other reachable
  only by absolute path. The address is now ambiguous rather than arbitrary, and the absolute path
  stays the escape hatch. `lookup` already refused ambiguous *suffix* matches; exact keys now match
  that behaviour.
- **`rata outline <file-ref>` had no `--profile`.** Once outline resolved through the ref space, a
  file pulled in by a profile could not be outlined at all. Added and threaded through.
- An empty or store-only ref (`memory:`, ``) produced *no* candidates, because similarity against an
  empty needle is zero for everything — the moment the reader most needs to be shown what exists.
  It now lists the store's nodes.

**Noted, not acted on:** `doctor` reads each store file several times (three store scans plus two
reads per node, ~45 ms over 19 nodes). Real, but a plumbing refactor to thread one `RefSpace` and one
outline set through `doctor` is disproportionate to a series already this large; recorded here rather
than rushed.

**Confirmed fine by the reviewer:** no reachable panics across a pathological fixture (multibyte
titles, unbalanced brackets, bare `@`, lone backtick, CRLF, invalid UTF-8); determinism of `pack`,
`outline`, `callers` and `graph`; cycle and multiple-parent handling.

### Checkpoints (memory boundaries)

- **PR-up:** —
- **Merge:** —
