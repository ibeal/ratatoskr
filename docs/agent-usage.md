# Agent Usage

Agents should usually do this:

1. `rata pack` on startup for default context
2. `rata pack --profile ...` for task-specific overlays
3. `rata resolve stores` when they need durable store paths

Common commands:

```text
rata pack
rata pack --profile build
rata pack --profile review
rata resolve stores
rata resolve stores --format json
rata resolve summary --profile build
```

Scope order is:

1. global
2. outer local scopes
3. inner local scopes

Profile selection is additive across all active scopes.

Global root precedence is:

1. `--global-root <path>`
2. `RATA_ROOT`
3. `~/.config/.rata/.rata.toml`
4. `~/.config/rata`
