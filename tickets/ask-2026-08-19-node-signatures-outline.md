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

### Fresh-eyes review (2026-08-19)

Subagent, fed only the diff + these AC + the review checklist. Findings applied:

**Blocking (both real, both fixed):**
- **A bad file killed the whole command.** An invalid-UTF-8 file, a `000`-mode directory, or a store
  path pointing at a file made `rata outline` *and* `rata doctor` exit 1 with nothing rendered —
  because `doctor` now scans stores. A diagnostic that dies on the thing it diagnoses is useless.
  Unreadable files are now nodes with an `unreadable` reason; unreadable directories become store
  `scan_issues`. Neither is fatal; both make `doctor nodes` unhealthy.
- **Symlinked directories were followed.** `path.is_dir()` follows links, so a self-referential
  symlink generated ~40 levels of refs and stopped only when the OS hit `ELOOP`; a link to
  `/nix/store` would have made a default scan unbounded. Now uses `entry.file_type()`, which does
  not follow, plus a 32-level cap on real nesting.

**Should-fix, applied:**
- **Tier 2 fired on non-prose.** `tickets:25 — Original:` and `install-helix — Phase: build Updated:
  2026-07-23` were both worse than the H1 sitting right there. Ordered lists and `+ ` items now
  count as structure, and a candidate is rejected if it ends in `:`, starts with a `Label:` token,
  or has fewer than two words. Re-running over the real tickets store turned every one of those
  into a real title.
- **`--depth 0` silently returned nothing.** Now rejected by clap (`1..`).
- **Layer shadowing was invisible.** A ref present in two layers dropped the loser with no trace.
  `Node.shadowed` records the shadowed paths and `doctor nodes` prints them.
- **`paths.dedup()` only collapsed adjacent duplicates**, so a repeat separated by an intermediate
  scope survived and the layer was scanned twice. Now a seen-set.
- **`tags:` was parsed but not validated** — `tags: nix` (scalar) silently produced `[]`. For a key
  reserved for later use, silent loss is the worst failure; a shape rata cannot read is now
  `Malformed`.
- **Unknown-key warnings covered the whole corpus.** `KNOWN_KEYS` is only `description`/`tags`, so
  every ticket-template and skill-manifest key drew a warning — and worse, `EAGERNESS_KEYS`
  contained `path`, `context`, `store`, `scope`, `root`, which are common enough in foreign
  frontmatter to hard-fail `doctor` on a whole repo. **The guardrail must not become the problem.**
  Those five words are gone from the list, and the unknown-key warning now fires only on a
  near-miss of a schema key (edit distance ≤ 2), which is typo detection rather than policing
  conventions rata does not own. Verified: zero warnings across the real five stores.
- `.MD` now matches; a filename containing `:` or `#` is unaddressable and reported rather than
  producing a broken ref; tier naming is `first-sentence` in both text and JSON; the four duplicate
  `temp_dir` test helpers are consolidated onto `test_support`.
- Added the missing tests: `strip_ref_prefix`, redundant rendering through `Display`, shadowing,
  unreadable files, symlink loops, ordered lists, label fragments.

**Confirmed fine by the reviewer:** UTF-8 safety of `truncate` and `cut_at_sentence_end` (verified
with multi-byte and emoji input), the `is_redundant`/`strip_ref_prefix` composition, determinism,
and empty/CRLF/no-trailing-newline handling.

**Not acted on:** the observation that this commit bundles two tickets. True, and deliberate — both
intakes recommended landing them together — and the commit message says so.

### Open questions

- ~~Does ladder tier 2 (first sentence) need a length cap, or is truncation at render time
  enough?~~ **Truncation at render time.** The ladder keeps the full sentence (so JSON consumers
  and later slices get the real text); the text renderer truncates at 120 chars on a word boundary.
  A cap in the ladder would silently lose data that has no other source.

### Checkpoints (memory boundaries)

- **PR-up:** —
- **Merge:** —
