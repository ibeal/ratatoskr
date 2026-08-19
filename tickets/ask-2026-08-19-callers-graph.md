# ask-2026-08-19-callers-graph — Reverse edges: `rata callers` + `rata graph`

| | |
|---|---|
| **Source** | plain ask |
| **Spec** | authored here |
| **Phase** | intake |
| **PR** | — |
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
- Add `rata graph [--format mermaid|dot] [--from <ref>] [--depth N]` for the occasional bird's-eye
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
- **Human's decision:** pending.

### Decisions carried in from design

- **Prose links count.** Frontmatter-only edges would miss nearly all real structure.
- **The verbs are the whole interface.** `outline` / `show` / `callers` / `graph`, mapping to module
  tree / read a function / go-to-definition / find-references. No TUI, no LSP.

### Build log

- 2026-08-19: Spec authored from design discussion. Not started.

### Open questions

- Exclude links inside fenced code blocks from edge extraction? (Leaning yes.)
- Should `callers` resolve transitively (`--depth`), or is one hop the useful answer?

### Checkpoints (memory boundaries)

- **PR-up:** —
- **Merge:** —
