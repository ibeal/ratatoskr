# ratatoskr

`ratatoskr` is a filesystem-first CLI for managing portable AI-agent context.

The project name is `ratatoskr`. The command is `rata`.

The name comes from Ratatoskr (`RAH-tah-toss-ker`), the squirrel in Norse mythology who runs along Yggdrasil, the world tree, carrying messages and insults between the eagle in its branches and Nidhogg at its roots. 

## Install

If you use Nix, you can run or install `rata` directly from this repo:

```text
nix run .#
nix build .#
nix profile install .#
```

From GitHub:

```text
nix run github:ibeal/ratatoskr
nix profile install github:ibeal/ratatoskr
```

For local development:

```text
nix develop
```

## Goal

Provide a small, agent-agnostic layer for:

- global context like `agents.md`, `preferences.md`, and shared workflow docs
- global datastores like `decisions/`, `memory/`, and `tickets/`
- local project context for architecture, tools, and repo-specific conventions
- local project datastores that live beside the code they describe

The first version is intentionally narrow. It discovers roots, scaffolds directory layouts, and
resolves the active context stack. It does not yet index content, search stores, or integrate with
specific agent products.

## Model

Ratatoskr resolves context in layers:

1. Global root: `~/.rata`
2. Local scopes: every ancestor directory containing `rata.toml`, found by walking upward from the current directory

Global root precedence is:

1. `--global-root <path>`
2. `RATA_ROOT`
3. nearest local `rata.toml` with `[settings].global_root`
4. `~/.rata/rata.toml` with `[settings].global_root`
5. `~/.rata`

Each root contains:

```text
rata.toml
  .rata/context/
  context/
  stores/
.rata/
  remotes/
```

The config file declares:

- ordered context file includes
- additive profiles that include additional context files
- remote files to fetch into a local cache
- settings like `allow_missing` and `global_root`
- named datastore directories

Settings compose in scope order too. `allow_missing` defaults to `true`, and later scopes can
override it. `global_root` can redirect which global root is used for a subtree or for the default
global root itself. `rata resolve` exposes the effective result plus a `settings_layers` trace so
you can see global and local values and where later scopes overrode earlier ones.

Remote files live in a separate `[remote_files]` section. They are fetched on a best-effort basis
before resolution. Fetch failures never raise on their own. If you reference a cached remote file
from `[context].include` and want that absence to fail, set `allow_missing = false`.
`rata doctor` reports health, active layers, and errors. Use `rata doctor stores` for store
composition diagnostics and `rata doctor settings` for effective settings plus their layers.

Remote defaults:

- `destination` defaults to `.rata/remotes/` next to the defining `rata.toml`
- `ttl` defaults to `-1`, which means never refetch if the cached file already exists

`rata init` also writes a Taplo schema directive at the top of generated `rata.toml` files so
editors can validate against the schema published in this repo at
`schema/rata.schema.json`.

Scopes compose in order: global first, then outer local scopes, then inner local scopes. Stores
default to `replace`, so existing configs retain their behavior. A store can instead expose all
matching layers using `global-first` or `local-first`; the nearest declaration that specifies a
composition selects it, allowing a local path declaration to inherit a global policy. The resolver
always returns paths in that order.

Profiles compose across scopes too. If global, `ap/`, and project scopes all define `build`, then
`rata resolve --profile build` activates all of them in scope order.

## Nodes, signatures, and outlines

Every `.md` file in a store is a **node** addressed by a store-relative ref without the extension:
`memory:containerized-agents`, `memory:nix/patterns`. A node has a one-line **signature** and a
**body**, and `rata outline` renders signatures only — a bird's-eye view you can step into.

The index is computed from a directory scan on every run. Nothing is written to disk, so no index can
go stale: add a file and it shows up in the next `rata outline`.

Signatures resolve through a fallback ladder, taking the first rung that yields something:

1. the optional frontmatter `description:` key
2. the first sentence of the body after the H1
3. the H1 heading text
4. the humanized filename

Frontmatter is never required. A file with none still gets a signature, which is why existing stores
need no backfill. `rata doctor nodes` reports the rung each node resolved at, so thin signatures are
visible without being mandatory to fix.

Frontmatter is a small, fixed schema:

```markdown
---
description: One line describing this file
tags: [nix, agents]
---

# Heading
```

`tags` is parsed and validated but not yet queryable; it is reserved so files written today do not
need rewriting when tag selection lands. Unrecognized keys are reported by `rata doctor nodes`
rather than ignored.

## Refs: one address for everything

```text
memory:containerized-agents            a store node
AGENTS.md#Safety                       a heading in a context file
workflow/sdlc.md#Phases/PR-summaries   a nested heading, path-addressed
memory:nix#Patterns                    a heading inside a store node
```

A **heading is a node too**, with the same signature/body model as a file: its body is the prose
under it *minus its descendants*, and its signature resolves through the same ladder (first sentence,
then the heading text). Inter-file navigation and intra-file navigation are the same operation at
different scales, which is why one syntax covers both.

`rata show <ref> [--depth N]` reads one node:

- `--depth 0` (the default) — the node's own body, plus the **signatures** of its children. You see
  what is below without paying for it.
- `--depth N` — descend N levels, bodies included.

The rule is uniform: a file's children are its top-level headings, so `show` behaves the same way
whichever granularity you address. A file with no headings has no descendants to exclude, so its own
body is the whole file.

`rata outline <file-ref>` renders that file's heading tree instead of a store's node list, so
`outline` and `show` operate over one model at both granularities. A bare name that matches a store
still means the store.

Addressing is stable where it matters. A heading may carry an explicit anchor:

```markdown
## PR summaries {#pr-sums}
```

The anchor is honoured in preference to the heading text, so a cross-referenced section keeps its
address when someone rewords the heading. Everything else falls back to heading-path addressing,
which costs nothing to author and is fragile only under renames. Segments match an anchor, the
slugified title, or the title itself, so `#PR-summaries` and `#PR summaries` both resolve.

A lone top-level heading is treated as the file's title rather than a section inside it — hence
`AGENTS.md#Safety`, not `AGENTS.md#agents-md-personal-operating-manual/Safety`.

An unresolvable ref fails with the closest candidates listed, never a bare error:

```text
error: unresolved ref `workflow/sdlk.md`; did you mean one of:
  AGENTS.md
  context/PREFERENCES.md
  workflow/sdlc.md
```

## Reverse edges: `callers` and `graph`

`outline` and `show` only descend. The links *between* documents make the real structure a graph, so
`rata callers <ref>` answers the question that has no other answer — *what depends on this?* — before
you edit or move something.

```text
rata callers context/PREFERENCES.md
rata callers 'AGENTS.md#Safety'
rata graph --format mermaid --from AGENTS.md --depth 2
rata graph --format dot
```

Edges come from **prose**, not from structured metadata, because that is where the structure
actually lives:

- markdown inline links, `[text](target)`
- reference definitions, `[label]: target`
- `@path` transclusions
- a path named in a code span — `` `workflow/sdlc.md` `` — since sentences like "read
  `workflow/sdlc.md` first" carry much of the graph

Links inside fenced code blocks are excluded: those are examples, not structure. A target resolves
either relative to the linking file or as a scope-relative ref, so both spellings work.

`callers` is **one hop**. Find-references is a one-hop question, and over a corpus this
cross-linked a transitive answer converges on "everything".

Given a heading ref, `callers` matches only links that reached that heading. Given a file ref, it
matches every link into the file.

Cycles and multiple parents are handled by design, not retrofitted: `graph --from` walks with a
visited set, so a cycle terminates, and a node reached by two parents is simply reached twice.

`rata doctor` reports a **broken edge** when an explicit link points at a markdown file that does not
exist, so the graph doubles as a dead-link check. A link to a file that exists but is not in a store
or an include is *not* reported — that is unaddressable, not dead, and conflating the two would make
`doctor` cry wolf.

`graph` is rendering only. No layout opinions: pipe the Mermaid or DOT output wherever you like.

## Store refs in `[context].include`

An `[context].include` entry may be a **store ref** — a bare store name and a colon — instead of a
path:

```toml
[context]
include = [
  "AGENTS.md",
  "context/PREFERENCES.md",
  "memory:",
]
```

`pack` renders that store's computed outline inline, at the same position and with the same section
framing a file include gets, marked as generated so a reader knows there is no source file behind it
and that editing the output changes nothing. Ordering is by ref, so two runs over an unchanged store
are byte-identical.

This replaces hand-maintained index files. A pointer list like `memory/MEMORY.md` has to be updated
every time a memory is added, and nothing enforces it; a store ref derives the same list from the
directory on every run. `rata doctor` **warns** when a store still contains a file that looks like a
hand-maintained index of its siblings, so the pattern does not quietly grow back.

A store ref naming a store no scope declares behaves like a missing file: `rata doctor` reports it,
and it is only fatal when `allow_missing = false`.

## Who decides what gets packed

Frontmatter introduces a second place that describes a file, alongside `rata.toml`. Two sources of
truth are fine, but only if the split is stated and enforced:

> **Frontmatter can never change whether a file is packed.**

- **`rata.toml` owns topology and eagerness** — which stores exist, where they live, what is a root,
  what each profile pulls. Facts that cannot be derived by scanning.
- **Frontmatter owns self-description** — `description`, later `tags`. Facts only the file knows.
  Never location, never profile membership.

The failure mode this prevents is a file opting *itself* into always-on context, so that adding a
memory silently bloats every session. Eagerness stays centralized. `rata doctor` **fails** — exit
code 2, not a warning — when it finds a frontmatter key that would affect eagerness, because the
symptom of a violation (context bloat) appears far from its cause.

Symmetrically, `rata.toml` never enumerates individual files *inside* a store: it names the store and
the contents self-describe. That is what keeps indexes generated rather than maintained.

One crossing is explicitly allowed. A **profile may reference a tag as a predicate** — e.g.
`store = "memory", tag = "nix"` — because the curator still decides; the tag is only the filter
expression. What is forbidden is a *file* pulling itself into a profile. (Inert until tags ship.)

### One grouping, two selectors

Stores, profiles, and tags are not three overlapping grouping mechanisms:

- **stores** are the filesystem — where bytes live. Not a selector.
- **profiles** select eagerly, for `pack`.
- **tags** select lazily, for `only` and `outline`.

The test for "should this be a store" is: *would you ever mount, sync, scope, or write to it
independently?* If not, it is a tag. Keep stores few and defined by where bytes must live, never by
subject matter; tags then do cross-cutting classification, including across stores.

Directory placement is self-evident and an unambiguous write target — "a new decision goes in
`decisions`" is checkable with `ls`. Tag correctness is not: a mis-tag makes a file *invisible*
rather than merely misfiled, which is a strictly worse failure. That asymmetry is why subject-matter
stores are not collapsed into one tagged store.

## Current commands

```text
rata init global
rata init local
rata --version
rata resolve summary
rata resolve stores
rata resolve --global-root ~/src/agent-context
rata resolve stores --format json
rata resolve --format json
rata doctor
rata doctor nodes
rata doctor nodes memory
rata doctor stores
rata doctor settings
rata doctor --format json
rata outline
rata outline memory
rata outline memory --depth 1
rata outline AGENTS.md
rata show memory:containerized-agents
rata show AGENTS.md#Safety
rata show 'workflow/sdlc.md#Phases/PR-summaries' --depth 1
rata callers context/PREFERENCES.md
rata graph --format mermaid --from AGENTS.md --depth 2
rata pack
rata only profile build
rata only scope local
rata only file agents.md
rata pack --format json
rata docs agent
```

You can also point the default global root somewhere else using `~/.rata/rata.toml`:

```toml
[settings]
global_root = "/Users/ian/src/agent-context"
```

And a local scope can override the global root for everything inside that subtree:

```toml
[settings]
global_root = "../../shared/work-context"
```

A nested layout like this is supported:

```text
~/src/
  ap/
    rata.toml
  ap/service-a/
    rata.toml
```

Running `rata resolve` inside `service-a` will compose:

1. global scope
2. `~/src/ap`
3. `~/src/ap/service-a`

## Example layout

`rata init global` creates the global root at `~/.rata/`.

`rata init local` creates only two root-level entries in the current directory:

- `rata.toml`
- `.rata/`

```text
~/.rata/
  rata.toml
  .rata/
    context/
      agents.md
      preferences.md
    remotes/
  stores/
    memory/

<repo>/
  rata.toml
  .rata/
    context/
      project.md
      tools.md
      standards.md
      review-checklist.md
    remotes/
    stores/
      decisions/
      memory/
      tickets/
```

## Example config

```toml
#:schema https://raw.githubusercontent.com/ibeal/ratatoskr/main/schema/rata.schema.json

version = 1

[context]
include = [
  ".rata/context/project.md",
  ".rata/context/tools.md",
  ".rata/remotes/architecture.md",
]

[settings]
allow_missing = true

[remote_files.architecture]
url = "https://example.com/architecture.md"
filename = "architecture.md"
ttl = -1

[profiles.build]
description = "Project-specific coding context"
include = [".rata/context/standards.md"]

[profiles.review]
description = "Project-specific review guidance"
include = [".rata/context/review-checklist.md"]

[stores]
decisions = ".rata/stores/decisions"
memory = { path = ".rata/stores/memory", composition = "local-first" }
tickets = ".rata/stores/tickets"
```

Store values may be a path string or an inline table with `path` and an optional `composition`.
When no scope specifies a composition, it defaults to `replace`. Composition controls only which
layers are returned and their order; individual store workflows decide how to handle duplicate
content.

## Next steps

- add `show stack` and `show context`
- add store helpers for recency-based reads and explicit named roots
