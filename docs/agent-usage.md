# Agent Usage

Agents should usually do this:

1. `rata pack` on startup for default context
2. `rata only profile ...` for task-specific overlays without refetching base context
3. `rata resolve stores` when they need durable store paths

Common commands:

```text
rata pack
rata only profile build
rata only profile review
rata only scope local
rata only file agents.md
rata resolve stores
rata resolve stores --format json
```

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

- `destination` defaults to `remote/` next to the defining `.rata.toml`
- `ttl` defaults to `-1`, which means never refetch if the cached file already exists

Global root precedence is:

1. `--global-root <path>`
2. `RATA_ROOT`
3. nearest local `.rata/.rata.toml` with `[settings].global_root`
4. `~/.config/rata/.rata.toml` with `[settings].global_root`
5. `~/.config/rata`
