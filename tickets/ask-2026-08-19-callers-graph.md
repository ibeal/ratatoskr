# ask-2026-08-19-callers-graph — Reverse edges: `rata callers` + `rata graph`

| | |
|---|---|
| **Source** | plain ask |
| **Spec** | authored here |
| **Phase** | build |
| **PR** | — (local stacked commit) |
| **Project(s)** | ratatoskr |
| **Created** | 2026-08-19 |
| **Updated** | 2026-08-19 |
| **Depends on** | `ask-2026-08-19-ref-space-show` |

---

## Spec — acceptance criteria

**Context.** `outline` and `show` only let you descend. The store layout is a tree, but the links
between documents make the real structure a graph — `context/PREFERENCES.md` is reachable from
several places, and `AGENTS.md` is mostly prose pointers to other files. Without reverse edges you
can only go down, which is the thing that makes flat markdown feel flat. This slice adds
find-references.

**Refined (after intake):**

- Add `rata callers <ref>` — every node whose body links to the given ref, with the linking line.
- Edge extraction follows **prose link targets and `@`-imports**, not just structured metadata.
  Markdown inline links, reference links, and `@path` transclusions all count.
- Handle the graph honestly: **cycles** must not hang or infinitely recurse, and a node may have
  **multiple parents**. Both designed in from the start, not retrofitted.
- Add `rata graph [--syntax mermaid|dot] [--from <ref>] [--depth N]` for the occasional bird's-eye
  picture. Rendering only — no layout opinions.
- Unresolvable link targets are reported by `rata doctor` as broken edges rather than silently
  dropped, so the graph doubles as a dead-link check.

**Explicitly out of scope:**

- TUI browser and markdown LSP. Both ruled out as overkill — the verbs are enough.
- ctags emission. Noted as the cheap 80% of editor-native navigation if the verbs ever prove
  insufficient; not built until then.

**Verification:** `rata callers context/PREFERENCES.md` finds the references from `AGENTS.md` and
`workflow/sdlc.md`; a deliberately introduced cycle terminates; `rata doctor` flags a deliberately
broken link.

---

## Journal

### Intake — "should we build this?"

- **Alignment:** this is the verb that makes the model a graph rather than a tree, and it is the one
  with no equivalent today — there is currently no way to ask "what depends on this context file?"
  before editing it.
- **Alternatives considered:** frontmatter-declared edges only (structured and unambiguous, but
  would miss almost the entire real graph, since most of the structure lives in sentences like
  "read `workflow/sdlc.md` first"); ripgrep (works, but no ref resolution, no cycle handling, and
  can't answer heading-granular questions).
- **AC sanity:** prose-link extraction will produce some noise — links in example blocks or quoted
  material. Decide whether fenced code blocks are excluded from edge extraction; leaning yes.
- **Recommendation:** go, lowest priority of the four. Its value scales with the size of the graph,
  and the graph is currently small.
- **Human's decision:** 2026-08-19 — go, last in the series; stacked local commit, no PR.

### Decisions carried in from design

- **Prose links count.** Frontmatter-only edges would miss nearly all real structure.
- **The verbs are the whole interface.** `outline` / `show` / `callers` / `graph`, mapping to module
  tree / read a function / go-to-definition / find-references. No TUI, no LSP.

### Build log

- 2026-08-19: Spec authored from design discussion. Not started.
- 2026-08-20: Built as `src/graph.rs`, on top of the ref space from the previous slice.
  `rata callers <ref>` and `rata graph [--syntax mermaid|dot] [--from <ref>] [--depth N]`.
- 2026-08-20: **Code spans had to count as edges.** Running the AC's own verification failed at
  first: `rata callers context/PREFERENCES.md` found `AGENTS.md` but not `workflow/sdlc.md`, because
  sdlc.md's only mention of that file is a *backticked path in a table*, not a markdown link. The
  spec's list (inline links, reference links, `@`-imports) does not cover that — but the Intake
  rationale names exactly this shape as the motivating case ("most of the structure lives in
  sentences like 'read `workflow/sdlc.md` first'"). So a code span counts, held to a tight shape (no
  whitespace, no glob characters, `.md` extension) to stay low-noise. With that, the AC verification
  passes and finds three further real references the spec did not anticipate.
- 2026-08-20: **Two spellings of a target both resolve.** A code span like `context/PREFERENCES.md`
  in a file two directories down is written scope-relative, not relative to the linking file. Link
  targets are now tried both ways: relative to the source, then as a ref-space address.
- 2026-08-20: **"Broken" needed a stricter definition, and this one mattered.** The first version
  reported a broken edge whenever a target was not in the ref space — which flagged eleven links in
  the real corpus, every one of them pointing at a file that *exists* (`workflow/_TICKET_TEMPLATE.md`,
  `context/PREFERENCES.container.md`, …) but is deliberately not in a store or an active include.
  A dead-link check that cries wolf about correct links is worse than none. A broken edge now
  requires an **explicit** link (not a code-span mention) to a `.md` path that **does not exist on
  disk**. Real corpus is clean; a deliberately dead link is still caught.
- 2026-08-20: Cycles and multiple parents are handled in `reachable` with a visited set. The real
  corpus already contains a cycle (`AGENTS.md` ⇄ `workflow/sdlc.md`) and `graph --from` terminates
  on it.

### Open questions

- ~~Exclude links inside fenced code blocks from edge extraction?~~ **Yes.** Fenced content is
  examples and quoted material; counting it would make every code sample an edge.
- ~~Should `callers` resolve transitively (`--depth`), or is one hop the useful answer?~~ **One hop,
  no `--depth`.** Find-references is a one-hop question, and this corpus is cross-linked densely
  enough (`AGENTS.md` ⇄ `sdlc.md` ⇄ `PREFERENCES.md`) that a transitive answer converges on
  "everything". `graph --from --depth` covers the multi-hop case, which is the one where a picture
  actually helps.

### Beyond the spec

- **Heading-granular callers.** Given `AGENTS.md#Safety`, `callers` matches only links that reached
  that heading; given a file ref, every link into the file. This is the thing intake noted ripgrep
  cannot do.
- Not done: attributing a *caller* to the heading it was written under. The source side stays
  file-granular plus a line number, which is what the AC asked for.

### Fresh-eyes review (2026-08-20)

**Blocking:**
- **`callers` and `graph --from` returned a confident zero for any non-canonical spelling of a ref.**
  Edges store the *canonical* ref, but matching compared against the address as typed. So
  `callers memory/MEMORY.md` reported 0 while `callers memory:MEMORY` reported 7 — and
  `memory/MEMORY.md` is the spelling that appears in `rata.toml`. For a find-references tool, a
  wrong "nothing links to this" is worse than an error, because nothing prompts you to doubt it.
  Both commands now canonicalize through the ref space first; every valid spelling gives the same
  answer.

**Should-fix, applied:**
- **Link extraction dropped real links on three ordinary shapes.** The `]` closing an inline link
  was found by first-match, so a linked image (`[![alt](pic.png)](target)`) or a nested label lost
  the outer target; and an unbalanced `[` or backtick hit `break`, abandoning every link later on
  that line. Bracket matching is now depth-aware, an unbalanced delimiter is treated as prose, and
  scanning resumes inside link text so nested links are found too.
- An image (`![alt](x.md)`) is no longer an edge — it is not something to navigate to — and an
  escaped `\[` is literal text.
- **One line naming the same target twice produced two callers.** A code span and an explicit link
  to the same file on one line is one reference; edges are now deduped per (from, to, line).
- `node_id` collided for genuinely distinct refs (`a-b.md` and `a_b.md` both sanitized to
  `n_a_b_md`), emitting duplicate diagram nodes. A short hash of the real ref is appended.
- **`graph` reused `--format` for the diagram language**, so it was the only command where
  `--format json` failed even though `GraphReport` already derived `Serialize`. The diagram language
  moved to `--syntax`; `--format text|json` now means what it means everywhere else.
- The `hand_maintained_index` heuristic counted `../elsewhere/nix.md` as a link to the store's own
  `nix.md`. Tightened.

**Fixed earlier in the series but worth recording here:** the synthesized store index told readers
to `rata only file <name>.md` while listing refs like `memory:nix` — a command that cannot take a
ref. Since that text lands in every agent's packed context, it was a live wrong instruction; it now
points at `rata show <ref>`.

### Checkpoints (memory boundaries)

- **PR-up:** —
- **Merge:** —
