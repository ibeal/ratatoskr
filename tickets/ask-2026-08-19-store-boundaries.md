# ask-2026-08-19-store-boundaries — Encode the config/frontmatter authority rule

| | |
|---|---|
| **Source** | plain ask |
| **Spec** | authored here |
| **Phase** | build |
| **PR** | — (local stacked commit) |
| **Project(s)** | ratatoskr, dotfiles/agents |
| **Created** | 2026-08-19 |
| **Updated** | 2026-08-19 |
| **Related** | `ask-2026-08-19-node-signatures-outline` |

---

## Spec — acceptance criteria

**Context.** Adding frontmatter introduces a second place that describes a file, alongside
`rata.toml`. Two sources of truth is fine — .NET has `.sln` and per-project files — but only if the
split is stated and enforced. Without it, "who decides what gets packed" becomes ambiguous exactly
when the answer matters most.

**The invariant:**

> **Frontmatter can never change whether a file is packed.**

- **`rata.toml` owns topology and eagerness** — which stores exist, where they live, what's a root,
  what each profile pulls. Facts that cannot be derived by scanning.
- **Frontmatter owns self-description** — `description`, later `tags`. Facts only the file knows.
  Never location, never profile membership.

The failure mode this prevents: a file opting *itself* into always-on context, so adding a memory
silently bloats every session. Eagerness stays centralized, always.

Symmetrically, `rata.toml` never enumerates individual files *inside* a store — it names the store
and the contents self-describe. That is what keeps indexes generated rather than maintained.

**Refined (after intake):**

- Document the invariant in the schema docs and in `context/rata.md`, in the terms above.
- `rata doctor` fails (not warns) if a frontmatter key is found that would affect eagerness — this is
  the enforcement point, and it should be impossible to violate accidentally.
- One allowed crossing, explicitly permitted: a **profile may reference a tag as a predicate** (e.g.
  `store = "memory", tag = "nix"`). The curator still decides; the tag is only the filter expression.
  What is forbidden is a *file* pulling itself into a profile. Document the distinction so it isn't
  later mistaken for a violation. (Inert until tags ship.)
- Record the conceptual model in the `decisions` store: it is **one grouping plus two selectors**,
  not three overlapping grouping mechanisms —
  - **stores** = the filesystem; where bytes live. Not a selector.
  - **profiles** select eagerly (`pack`).
  - **tags** select lazily (`only`, `outline`).

**Explicitly out of scope:**

- Implementing tags. Deferred.
- Merging any stores. That's a dotfiles decision needing no rata change; see Open questions.

**Verification:** `rata doctor` rejects a fixture file whose frontmatter attempts to set eagerness;
the docs state the invariant in one sentence.

---

## Journal

### Intake — "should we build this?"

- **Alignment:** this is the guardrail that makes the frontmatter work in the other tickets safe to
  ship. Cheap now, expensive to retrofit once files are in the wild.
- **Alternatives considered:** convention-only (document it, don't enforce) — rejected, because the
  violation is silent and its symptom (context bloat) appears far from its cause.
- **AC sanity:** nothing today violates the rule; this is prophylactic. That is the right time.
- **Recommendation:** go, and do it alongside the signature-ladder slice rather than after.
- **Human's decision:** 2026-08-19 — go on the invariant. Separately: **keep the existing five store
  boundaries.** Merging `decisions` and `memory` and introducing tags are to be explored together,
  later, as one change — not piecemeal. Until then this ticket ships the guardrail only.

### Decisions carried in from design

- **Stores are physical, tags are logical.** The test for "should this be a store" is: *would you
  ever mount, sync, scope, or write to it independently?* Applied to the current five:
  - `tools` — mounted into containers at `/tools` by `run.toml`. A tag cannot be mounted. Decisive.
  - `skills` — the agent harness discovers skills by directory. External consumer forces it.
  - `tickets` — write target with its own lifecycle.
  - `decisions` + `memory` — both "durable prose Ian wrote"; the boundary between them is
    *decision vs lesson*, a what-it's-about distinction. **The one real merge candidate.**
- **A store is also an unambiguous write target.** "New decision → `decisions`" is checkable with
  `ls`. Under one-store-plus-tags it becomes "one directory, tag it right" — and a mis-tag makes a
  file *invisible* rather than merely misfiled, a strictly worse failure. Directory placement is
  self-evident; tag correctness requires reading everything.
- **Therefore: do not collapse to a single tagged store.** Keep stores few and defined by where
  bytes must live, never by subject matter. Tags then do cross-cutting classification, including
  *across* stores — which is the thing the current taxonomy genuinely cannot do.
- **All five stores stay as they are** (2026-08-19). The `decisions`/`memory` merge is a live idea,
  not a rejected one, but it is coupled to tags: merging without them loses the lesson/decision
  distinction entirely. Both get explored together or not at all.

### Build log

- 2026-08-19: Spec authored from design discussion. Not started.
- 2026-08-19: Built alongside the signature-ladder slice, as recommended.
  - **Enforcement:** `rata doctor` now reads the frontmatter of every resolved context file *and*
    every store node, and reports a `frontmatter_eagerness` error for any key on the forbidden list
    (`always`, `context`, `eager`, `include`, `pack`, `path`, `profile`, `profiles`, `root`,
    `scope`, `store`). Checking context files too matters — the invariant is about any markdown
    rata reads, not only store contents.
  - **`doctor` now exits non-zero.** It previously always exited 0, which made "fails, not warns"
    unachievable. Any unhealthy report (including the pre-existing missing-context-file and
    remote-fetch errors) now exits **2**, so a hook or CI step can gate on it. This is a behaviour
    change to an existing command, deliberate and required by the AC.
  - **Docs:** the invariant, the allowed tag-as-predicate crossing, and the one-grouping-plus-two-
    selectors model are stated in `README.md` and `docs/agent-usage.md`, and the `rata.toml` JSON
    schema now carries it in its top-level, `profiles`, and `stores` descriptions — so it shows up
    in editor tooltips at the point of temptation.

### Not done (deliberately, and why)

- **`context/rata.md`** and a **`decisions` store record** both live in `~/dotfiles/agents`, which
  this series is not touching (2026-08-19 decision: ratatoskr only). The conceptual model is
  recorded in this repo's `README.md` instead. Copying it into the `decisions` store and adding the
  invariant to `context/rata.md` are left for Ian.

### Open questions

- None. The `decisions`/`memory` merge is **decided: keep the existing store boundaries** (see
  Intake). Revisit alongside tags, not before.

### Checkpoints (memory boundaries)

- **PR-up:** —
- **Merge:** —
