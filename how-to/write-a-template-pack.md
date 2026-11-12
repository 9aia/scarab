# Write a template pack

A template pack is a tamias package with `type: template` whose
`tamias.yaml` carries a `template:` table. The
`tamias-driver-template` driver renders each declared `src` through
the engine and writes it at `into`.

## Pack layout

```
repo-bootstrap/
├── tamias.yaml
├── app.toml.tmpl
└── notes.md.tmpl
```

## Manifest

```yaml
name: repo-bootstrap
type: template
version: "0.1.0"

template:
  vars:
    env: dev # defaults — lowest-precedence layer
  files:
    - src: app.toml.tmpl
      into: ./config/app.toml
    - src: notes.md.tmpl
      into: docs/NOTES.md
```

- `template.vars` — flat `name → value` defaults (any JSON value:
  strings, numbers, bools, lists, tables). Overridable per deploy.
- `template.files[]` — `src` is a path inside the pack payload,
  `into` the render target relative to the deploy target dir.

Both `src` and `into` must be safe relative paths: absolute paths,
empty paths, and `..` components are rejected at validation time —
before anything is written.

## Template syntax

The engine is [minijinja](https://github.com/mitsuhiko/minijinja) —
`{{ var }}`, `{% if %}`, `{% for %}`. It is sandboxed: pack templates
are third-party content and cannot reach the filesystem or network.
Undefined variables render as empty.

```jinja
# app.toml.tmpl
env = "{{ env }}"
repo = "{{ repo }}"
user = "{{ user }}"
```

## Inherit from a parent pack

`template.extends` names another template pack by tamias ref — the
child renders the parent's files too, with the child's `into`s and
`vars` winning:

```yaml
template:
  extends: git:9aia/scarab//libraries/rust-repo
  vars:
    channel: nightly # overrides the parent's "stable" default
  files:
    - src: extra.toml.tmpl
      into: ./extra.toml
```

`tamias install` resolves the ref and materializes the parent's
payload inside yours at `.scarab-parent/` (recursively for
grandparents). The driver renders ancestor-first: parent files
write first, yours last — on an `into` conflict **you win**, and the
state digest records your version. See
[template packs — composition](/reference/template-packs#composition)
for the full contract, including the vendored-parent case
(`.scarab-parent/` present without `extends`).

## Variable layers

Later layers win:

1. ancestor `template.vars` defaults (extends chain, ancestor-first),
   then this pack's `template.vars`
2. auto vars — `repo` (git remote basename or dir name of the target
   repo) and `user` (`$USER`/`$LOGNAME`)
3. `--config` vars file (JSON / TOML / YAML by extension)
4. `$TEMPLATE_VAR_<NAME>` env vars (lowercased name)
5. trailing `key=value` args

See [deploy a template pack](/how-to/deploy-a-template-pack) for the
full invocation.

## Validate before shipping

`tamias-driver-template install --dir <pack> --name <pkg>` parses the
manifest, checks every `src` exists and compiles, resolves the var
context, and previews the render plan — writing nothing. Run it
against your pack dir before `tamias publish`.
