# Scarab

*Config & template expertise — generic config loading and templated
content packs.*

Named for *Scarabaeus sacer* — the sacred scarab that rolls the ball
into place; scarab amulets were seals and signets — config as the
seal that validates what runs.

Scarab owns the part every 9aia repo kept rewriting: loading typed
configuration from files (discovery + TOML/YAML formats, generic
over the model) — and, later, templated content distributed as
tamias packages.

## Layout

Ports-and-adapters, flat `crates/` workspace. Ports are traits,
impls are separate crates; `xtask` is the composition root for
repo chores.

| Crate | Role |
| --- | --- |
| `scarab-config` | `ConfigSource<T>`/`ConfigFormat<T>` traits + `Discovery` spec — generic over the config model, no I/O |
| `scarab-config-file` | file-discovery impl — explicit path → `$<NAME>_CONFIG` → `./<name>.<ext>` → `~/.config/<name>/`; YAML preferred on ties |
| `scarab-config-toml` | TOML `ConfigFormat` — serde parse/render for any model |
| `scarab-config-yaml` | YAML `ConfigFormat` — claims `.yaml`/`.yml` |
| `scarab-template` | template port — `render(text, ctx)` + `TemplateContext` merge, safe-path validation |
| `scarab-template-minijinja` | MiniJinja `TemplateEngine` impl — `{{ }}` vars + `{% %}` blocks |
| `tamias-driver-template` | `type: template` content driver — install validates, deploy renders pack payloads to target paths, uninstall removes |

A consumer keeps only its serde `Config` model:

```rust
let cfg: MyConfig = FileSource::for_app("myapp", "MYAPP_CONFIG")
    .load()?;
```

## `type: template` packs

Template packs are tamias packages whose payload is rendered at
install/deploy time. The pack's `tamias.yaml` carries a
`template:` section (driver-specific keys ride alongside the
standard manifest):

```yaml
name: repo-bootstrap
type: template
version: "0.1.0"

template:
  vars:
    env: dev            # defaults, overridable per deploy
  files:
    - src: app.toml.tmpl
      into: ./config/app.toml
    - src: notes.md.tmpl
      into: docs/NOTES.md
```

Variable layers (later wins): `template.vars` defaults → auto vars
(`repo`, `user`) → `--config` file (json/toml/yaml) →
`TEMPLATE_VAR_<NAME>` env → trailing `key=value` args.

```bash
tamias install git:9aia/scarab//libraries/repo-bootstrap
# driver dispatches install/deploy/uninstall/config-schema/capabilities
# like every tamias content driver
```

Targets must be safe relative paths — absolute paths and `..` are
rejected. `deploy` records sha256 digests in
`.scarab-template-state.json`; modified files are left alone and
reported; `uninstall` removes only files still matching their
recorded digest.

## Commands

```bash
cargo xtask check          # fmt + clippy -D warnings
cargo xtask test           # whole suite (nextest if present)
cargo xtask coverage       # llvm-cov report; extra args pass through
cargo xtask package        # cargo package --workspace (verify)
```

Git hooks live in `.hooks/` (`core.hooksPath` = `.hooks`).

## Boundary

- Scarab owns **config loading + (later) templated content** —
  never names a consumer's `Config` type.
- **tamias** distributes template/config packs; **cuora** owns
  secret values (config carries secret *names*, never values);
  **tarsi** consumes the libs for `tarsi.toml`/profile loading.

Design history: org ledger proposal P-017 (supersedes P-015).
