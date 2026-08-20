# Agent Usage

Agents should usually do this:

1. `rata pack` on startup for default context
2. `rata only profile ...` for task-specific overlays without refetching base context
3. `rata outline <store>` to see what a store holds before reading any of it
4. `rata resolve stores` when they need durable store paths

Common commands:

```text
rata pack
rata only profile build
rata only profile review
rata only scope local
rata only file agents.md
rata outline
rata outline memory
rata outline memory --depth 1
rata resolve stores
rata resolve stores --format json
```

## Outlines and signatures

`rata outline` computes a store's index from a directory scan at read time. There is no index file,
so there is nothing to keep in sync — a new `.md` file appears in the next outline with no other
edit.

Every node gets a one-line **signature** from the first rung of this ladder that produces something:

1. the frontmatter `description:` key
2. the first sentence of the body after the H1
3. the H1 heading text
4. the humanized filename

Frontmatter is optional at every level; files with none still get a usable signature. When the
signature would only restate the ref, the ref is rendered alone. Use `rata doctor nodes` to see which
rung each node landed on, which is how thin signatures stay visible without `description:` ever being
required.

Frontmatter recognizes `description` and `tags`. `tags` is parsed and validated but not yet
queryable — it is reserved so files written now do not need rewriting later.

## Store refs in the pack

A `[context].include` entry ending in a colon — `memory:` — is a store ref, not a path. `rata pack`
renders that store's outline inline where the entry sits, labelled `## Store Index: memory:` and
marked `generated:`. There is no file behind it: do not try to open or edit it, and do not treat a
missing `MEMORY.md`-style index file as a problem. Add a memory and it appears in the next `pack`.

## The one frontmatter invariant

> **Frontmatter can never change whether a file is packed.**

`rata.toml` owns topology and eagerness (which stores exist, where they live, what each profile
pulls). Frontmatter owns self-description only (`description`, later `tags`) — never location, never
profile membership.

So: when adding a memory or a decision, write `description:` if the first sentence would not make a
good signature, and nothing else. Do **not** try to make a file always-on from inside the file;
that is a `rata.toml` edit. `rata doctor` exits 2 if it finds a frontmatter key that would affect
eagerness.

Stores, profiles, and tags are one grouping plus two selectors, not three groupings: stores are where
bytes live, profiles select eagerly for `pack`, tags select lazily for `only` and `outline`.

Use `only` when the agent already has the base pack and just needs an extra slice:

```text
rata only profile build
rata only scope local
rata only file agents.md
```

Scope order is:

1. global
2. outer local scopes
3. inner local scopes

Profile selection is additive across all active scopes.

Settings are composable too:

- `allow_missing` defaults to `true`
- `global_root` can redirect which global root is active

Remote files are best-effort caches. If a scope defines them, `rata` will try to refresh them before
resolution, but fetch failures are ignored. A missing cached remote only becomes fatal later if a
referenced file is still absent and `allow_missing = false`.

Remote defaults:

- `destination` defaults to `.rata/remotes/` next to the defining `rata.toml`
- `ttl` defaults to `-1`, which means never refetch if the cached file already exists

Generated `rata.toml` files start with a Taplo `#:schema` directive pointing at the JSON Schema
published from this repo, so editor validation can work without additional local setup.

Global root precedence is:

1. `--global-root <path>`
2. `RATA_ROOT`
3. nearest local `rata.toml` with `[settings].global_root`
4. `~/.rata/rata.toml` with `[settings].global_root`
5. `~/.rata`
