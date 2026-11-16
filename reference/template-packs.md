# Template packs

A template pack is a tamias package with `type: template` whose
`tamias.yaml` carries a `template:` table. The port lives in
`scarab-template` (pure domain — no I/O, no engine specifics); impls
own the syntax.

## Manifest schema

```yaml
name: repo-bootstrap
type: template
version: "0.1.0"

template:
  vars: # optional — flat name → JSON value map
    env: dev
    replicas: 1
  extends: git:9aia/base//libraries/base-pack # optional — parent ref(s)
  merge: # optional — composition rules (defaults shown)
    vars: child-wins    #   child-wins | parent-wins
    into: child-wins    #   child-wins | parent-wins | error
    max-depth: 8        #   ancestor nesting cap (>= 1)
  files: # required, non-empty
    - src: app.toml.tmpl # path inside the pack payload
      into: ./config/app.toml # render target, relative to target dir
```

### `template.vars`

`BTreeMap<String, serde_json::Value>` — defaults layer, lowest
precedence. Values may be strings, numbers, bools, arrays, or tables;
templates see them as engine-native values.

### `template.files[]`

| Field  | Meaning                                          |
| ------ | ------------------------------------------------ |
| `src`  | Template path inside the payload root            |
| `into` | Render target, relative to the deploy target dir |

`into` must normalize to a non-empty relative path — absolute paths,
`..`, root/prefix components, and empty results are
`TemplateError::UnsafeTarget`. `src` follows the same rule inside the
payload (`TemplateError::UnsafeSource`).

### `template.extends`

Optional parent declaration(s) — one or several tamias refs
(`git:org/repo//path`, `cf:…`, a store name, …). Three spellings,
all normalizing to an ordered parent list **lowest precedence
first**:

```yaml
extends: git:9aia/base//libraries/base-pack            # one parent
# extends: { source: git:9aia/base//x, via: tamias-ref }  # structured
# extends:                                             # several parents
#   - git:9aia/platform
#   - source: corp/overlay
#     via: tamias-ref
```

`source` is an opaque locator — its grammar belongs to the resolver
(`via` names the scheme; absent/`tamias-ref` = the ambient tamias
resolver). An unknown `via` is a resolution error, never a silent
fallback. See [Composition](#composition) below.

### `template.merge`

Optional composition rules as data (defaults shown above). The
**installing (root) pack's** `merge` governs the whole chain —
ancestor packs' own `merge` tables describe how they compose when
installed directly.

- `vars` — pack-default precedence across the chain:
  `child-wins` (default, nearer packs win) or `parent-wins`.
- `into` — duplicate-target policy: `child-wins` (default, last
  render kept), `parent-wins` (first kept), or `error`.
- `max-depth` — ancestor-nesting cap (default 8, must be ≥ 1).

## Composition (`extends` chains)

A pack may inherit from other template packs. `tamias install`
resolves `template.extends` and materializes each ancestor's payload
inside the child's — `.scarab-parent/` for a single parent,
`.scarab-parents/<i>/` (index = declaration order) for several —
recursively, so a grandparent lands nested inside its parent.

The driver evaluates the chain **ancestor-first** (the agnostic
contract lives in `scarab-template`'s `compose.rs` +
`COMPOSITION.md`):

- parents render in declaration order, each parent's own ancestors
  before it; a shared ancestor renders **once** at its first
  occurrence (diamonds collapse)
- ancestor `template.vars` fold into the defaults ancestor-first —
  subject to `merge.vars`; the child's defaults then win under the
  default `child-wins`
- every chain entry's `files[]` renders against the one context;
  `into` conflicts resolve per `merge.into` (default: the
  descendant's render wins)
- the state file records only the final written digest per `into` —
  exactly what `uninstall` needs

Edge cases:

- materialized `.scarab-parent*/` present **without** `extends` is
  honored as vendored composition — useful for local packs and tests
- `extends` declared but fewer/more materialized parents than
  declared is an error: the payload never went through `tamias
  install` (or the layout is corrupt), and the driver can't fetch
  refs
- `extends: []` or a parent with an empty `source` fails manifest
  validation

## `TemplateEngine`

The engine port — one impl per syntax family:

```rust
pub trait TemplateEngine: Send + Sync {
    fn name(&self) -> &'static str;
    fn render(&self, source: &str, ctx: &serde_json::Value)
        -> Result<String, Box<dyn Error + Send + Sync>>;
}
```

`ctx` is a JSON object; engines map it to their native value.
`MinijinjaEngine` (`scarab-template-minijinja`) is the v1 impl —
sandboxed, keeps the template's trailing newline so rendered config
files stay POSIX-clean.

## Variable context

`merge_context(defaults, auto, file, args)` merges flat
`name → JSON value` layers, later winning:

```
ancestor template.vars … this pack's template.vars
    <  auto vars  <  --config file  <  env+args
```

On an `extends` chain the driver folds every ancestor's
`template.vars` into `defaults` ancestor-first, so nearer packs win.
The driver supplies the layers:

1. `template.vars` pack defaults — every ancestor's folded
   ancestor-first, then this pack's (nearest wins)
2. auto vars — `repo` (git `remote.origin.url` basename of the target
   dir, else canonical dir name) and `user` (`$USER`/`$LOGNAME`)
3. `--config` file — JSON by `.json`, TOML by `.toml`, YAML otherwise
   (incl. extensionless); must be a flat map
4. `$TEMPLATE_VAR_<NAME>` env vars, name lowercased, value parsed as
   JSON falling back to string
5. trailing `key=value` args — value parsed as JSON, falling back to
   string

## Errors

| Variant        | Meaning                                            |
| -------------- | -------------------------------------------------- |
| `Manifest`     | Pack manifest or `template:` table failed to parse |
| `UnsafeTarget` | `into` escaped the deploy dir                      |
| `UnsafeSource` | `src` escaped the payload root                     |
| `NotUtf8`      | Rendered output isn't valid UTF-8 text             |
