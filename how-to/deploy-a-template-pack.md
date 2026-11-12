# Deploy a template pack

Template packs deploy through tamias: `tamias install` fetches the
package and dispatches the `type: template` verbs to
`tamias-driver-template`. You can also run the driver binary directly
against a local payload dir.

## Via tamias

```bash
# the shipped example pack — starter configs for a new Rust workspace
tamias install git:9aia/scarab//libraries/rust-repo
```

Tamias calls the driver's `install` verb for validation, then
`deploy` for the actual render+write. `uninstall`/`remove` reverses
it.

## Directly

```bash
tamias-driver-template <verb> --dir <payload> --name <pkg> \
    [--config <vars-file>] [--target <dir>] [--dry-run] [key=value …]
```

| Verb            | What it does                                                                                                    |
| --------------- | --------------------------------------------------------------------------------------------------------------- |
| `install`       | Validate the pack and preview the render plan. Writes nothing.                                                  |
| `deploy`        | Render each `template.files[]` entry and write it at `into` under the target dir.                               |
| `uninstall`     | Remove files still matching the recorded sha256 digests.                                                        |
| `config-schema` | Print the JSON Schema for the `--config` vars file.                                                             |
| `capabilities`  | Print `{"name": "template", "types": ["template"], "verbs": […], "engine": "minijinja"}` — the discovery probe. |

On an `extends` chain, `install`/`deploy` render every ancestor pack
inside `.scarab-parent/` first — the child's files win on `into`
conflicts. See
[template packs — composition](/reference/template-packs#composition).

## Choosing the target dir

`deploy`/`uninstall` write relative to, in order:

1. `--target <dir>`
2. `$TEMPLATE_TARGET`
3. the current working directory

## Overriding variables

Vars resolve in layers (later wins): ancestor `template.vars`
defaults (extends chain) → this pack's `template.vars` → auto vars
(`repo`, `user`) → `--config` file → `$TEMPLATE_VAR_<NAME>` env →
`key=value` args.

```bash
# vars file — parsed as JSON/TOML/YAML by extension (default YAML)
tamias-driver-template deploy --dir ./repo-bootstrap --name repo-bootstrap \
    --config prod.yaml --target ~/myrepo

# env + inline overrides
env=prod TEMPLATE_VAR_replicas=3 \
tamias-driver-template deploy --dir ./repo-bootstrap --name repo-bootstrap \
    --target ~/myrepo replicas=5
```

`--dry-run` prints what would be written without touching the target.

## Uninstall semantics

`deploy` records `.scarab-template-state.json` at the target root —
each `into` path mapped to the sha256 of what was written.
`uninstall` deletes only files whose current digest still matches;
files modified since deploy are left in place and reported as kept.
State entries for already-removed files are dropped, and the state
file itself is removed once nothing remains.
