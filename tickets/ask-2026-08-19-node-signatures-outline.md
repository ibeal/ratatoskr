# ask-2026-08-19-node-signatures-outline — Node signatures + `rata outline`

| | |
|---|---|
| **Source** | plain ask |
| **Spec** | authored here |
| **Phase** | build |
| **PR** | — (local stacked commit) |
| **Project(s)** | ratatoskr |
| **Created** | 2026-08-19 |
| **Updated** | 2026-08-19 |

> One file, two artifacts: the **Spec** (the contract) and the **Journal** (what happened).
> See `workflow/sdlc.md`.

---

## Spec — acceptance criteria

*Foundation slice. The rest of this series depends on it.*

**Context.** Markdown context files are flat: an agent (or Ian) either loads a whole file or nothing.
The goal across this series is to make context navigable at two granularities the way code is —
bird's-eye, then step in. This slice supplies the primitive: every addressable node has a
**signature** (a one-line summary) and a **body**, and `outline` renders signatures only.

**Original ask:** move up and down the ladder of abstraction over markdown context, instead of
reading flat files.

**Refined (after intake):**

- Parse an **optional** `description:` frontmatter key on store files. Absence is never an error.
- Resolve every node's signature through a fallback ladder, in order:
  1. frontmatter `description:`
  2. first sentence of the body after the H1
  3. H1 heading text
  4. humanized filename
- When the resolved signature is substantially the same as the node's ref name, render the ref alone
  rather than `foo-bar — Foo bar`. No redundant output.
- Add `rata outline [<store>] [--depth N]`, computing the index from a directory scan at read time.
  There is no index file to maintain, so none can go stale.
- `rata doctor` reports which ladder tier each node resolved at, so thin signatures are visible
  without being mandatory to fix.
- Reserve `tags:` in the frontmatter schema — parsed and validated, not yet queryable — so files
  written before the tag work don't need rewriting.

**Explicitly out of scope:**

- Tag **querying** (`--tag`). Deferred; see Decisions below.
- Intra-file heading nodes. That's the ref-space slice; this one is file-granular.
- Any change to what `pack` emits. Separate slice.

**Verification:** `rata outline memory` and `rata outline decisions` produce a readable index with no
frontmatter added anywhere. Adding a new `.md` file makes it appear with no other edit.

---

## Journal

### Intake — "should we build this?"

- **Alignment:** rata's job is context resolution. Today `pack` is all-or-nothing and `only` requires
  you to already know what to ask for. A computed outline is the missing middle.
- **Alternatives considered:** hand-maintained index files — the status quo, and the thing to kill.
  Both `memory/MEMORY.md` and the `.claude` auto-memory index are hand-written pointer lists
  duplicating data the files already carry, and both can drift. A marker-block generator writing
  into a real file was also considered; keep it as the escape hatch if a non-rata consumer ever
  needs the index on disk.
- **AC sanity:** `memory/` has no frontmatter at all today (`containerized-agents.md` opens straight
  into its H1). That is exactly why `description:` must be optional — the ladder has to produce
  something useful for files as they already exist, with zero backfill.
- **Recommendation:** go.
- **Human's decision:** 2026-08-19 — go. AC confirmed as written; build the series in order, as
  stacked local commits rather than PRs.

### Decisions carried in from design

- **Optional, not required.** A mandatory `description:` would turn a feature into a migration.
- **The index is a view, not a file.** Derived data is not stored. That is the whole answer to "the
  index is dynamic because memories get added all the time."
- **Tags deferred.** The memory store is two files; tag queries over a store that fits on one screen
  are ceremony. Confirmed 2026-08-19: store boundaries stay as they are for now, and tags plus the
  `decisions`/`memory` merge get explored together as one later change (see
  `ask-2026-08-19-store-boundaries`) — merging without tags would lose the lesson/decision
  distinction. Only `tags:` schema reservation lands in this slice.

### Build log

- 2026-08-19: Spec authored from design discussion. Not started.
- 2026-08-19: Built. New `src/frontmatter.rs` (optional `description:` + reserved `tags:`, never
  fails — issues surface in `doctor`) and `src/outline.rs` (directory scan → nodes → signature
  ladder → `rata outline [<store>] [--depth N]`). `rata doctor nodes [<store>]` reports the tier
  each node resolved at plus a per-tier count.
- 2026-08-19: Two ladder refinements came out of running it against the real stores, not from the
  spec:
  - **List continuations are not prose.** `memory/containerized-agents.md` is entirely a bullet
    list; its wrapped continuation lines were being read as the first sentence, yielding a
    mid-clause fragment. Indented lines before any prose has started are now treated as belonging
    to the structure above them, so that file correctly falls through to the heading tier.
  - **Headings that restate the filename get the prefix stripped.** The tickets store's H1
    convention is `<id> — <title>`, which rendered as `<ref> — <ref> — <title>`. Exact-match
    redundancy detection did not catch it, so the heading tier now drops a leading segment that
    squashes equal to the ref.
- 2026-08-19: Store layers are deduped before scanning — a root that is both the global root and a
  local scope (which is exactly `~/dotfiles/agents`) otherwise contributes every layer twice.

### Open questions

- ~~Does ladder tier 2 (first sentence) need a length cap, or is truncation at render time
  enough?~~ **Truncation at render time.** The ladder keeps the full sentence (so JSON consumers
  and later slices get the real text); the text renderer truncates at 120 chars on a word boundary.
  A cap in the ladder would silently lose data that has no other source.

### Checkpoints (memory boundaries)

- **PR-up:** —
- **Merge:** —
