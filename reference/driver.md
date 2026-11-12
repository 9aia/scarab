# Template driver

`tamias-driver-template` is the tamias content driver for
`type: template` packs — it renders a pack's files into declared
targets. Discovered by tamias as `tamias-driver-template` on
`PATH`/`drivers/` like every content driver.

## argv contract

Tamias verbs always arrive as:

```
tamias-driver-template <verb> --dir <payload> --name <pkg> \
    [--config <vars-file>] [--target <dir>] [--dry-run] [key=value …]
```

| Flag        | Meaning                                                                                  |
| ----------- | ---------------------------------------------------------------------------------------- |
| `--dir`     | Pack payload dir (contains `tamias.yaml`/`.yml`) — required for install/deploy/uninstall |
| `--name`    | Package name — required for install/deploy/uninstall                                     |
| `--config`  | Vars file — JSON/TOML/YAML by extension, YAML default                                    |
| `--target`  | Deploy target dir — overrides `$TEMPLATE_TARGET` and cwd                                 |
| `--dry-run` | Print actions without writing (deploy/uninstall)                                         |
| `key=value` | Trailing var overrides, highest-precedence layer                                         |

## Verbs

| Verb            | Behavior                                                                                                                                                                           | Exit      |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------- |
| `install`       | Validate the pack _chain_ (each manifest parses, every `src` compiles, vars resolve); print the merged render plan. Writes nothing — the payload dir is the evictable fetch cache. | 0 / 1 / 2 |
| `deploy`        | Render each `template.files[]` entry across the whole `extends` chain (ancestors first — the child wins on `into` conflicts), write at `into` under target dir, record state.      | 0 / 1 / 2 |
| `uninstall`     | Remove files still matching recorded sha256 digests; modified files kept + reported.                                                                                               | 0 / 1 / 2 |
| `config-schema` | Print JSON Schema (draft 2020-12) for the `--config` vars file — a flat `name = value` map.                                                                                        | 0         |
| `capabilities`  | Print `{"name": "template", "types": ["template"], "verbs": ["install","deploy","uninstall","config-schema"], "engine": "minijinja"}`.                                             | 0         |

Exit codes: `0` success, `1` operational error (message on stderr —
tamias wraps it as `driver … failed: …`), `2` usage/arg error.

## `extends` chains

`install` and `deploy` walk the pack's chain: each level's payload
may nest its parent at `.scarab-parent/` (materialized by
`tamias install` from `template.extends`). `uninstall` needs no
chain — it reverses whatever the state file recorded.

- `install` reports the merged plan — every pack's `into`s in render
  order, with a count of files overridden by a later pack
- `deploy` renders ancestor-first; on an `into` conflict the child
  (later pack) writes last and its digest is what state records
- `extends` declared but `.scarab-parent/` missing → error ("install
  through `tamias install`"); `.scarab-parent/` present without
  `extends` → honored as a vendored parent
- var layers fold ancestor `template.vars` first, so nearer packs
  win — see [template packs](/reference/template-packs#composition)

## State file

`deploy` writes `.scarab-template-state.json` at the target root:

```json
{
  "pack": "repo-bootstrap",
  "files": {
    "config/app.toml": "<sha256 hex>",
    "docs/NOTES.md": "<sha256 hex>"
  }
}
```

The file is shared per target dir — `deploy` merges its entries into
any existing state (multiple packs can coexist). `uninstall` removes
entries as files are deleted or found missing; the state file itself
is removed once `files` is empty.

## Driver errors

All failures surface as `OpsError` — a readable message on stderr:

- missing `--dir`/manifest, `type` isn't `"template"`, empty
  `template.files`, bad `template:` table shape
- unsafe `src`/`into` paths (rejected before any write)
- missing/unreadable payload files, render failures
- malformed `--config` vars file (must parse to a map)
- I/O errors on read/write/remove
