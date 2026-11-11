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
  vars:                      # optional — flat name → JSON value map
    env: dev
    replicas: 1
  files:                     # required, non-empty
    - src: app.toml.tmpl     # path inside the pack payload
      into: ./config/app.toml  # render target, relative to target dir
```

### `template.vars`

`BTreeMap<String, serde_json::Value>` — defaults layer, lowest
precedence. Values may be strings, numbers, bools, arrays, or tables;
templates see them as engine-native values.

### `template.files[]`

| Field | Meaning |
| --- | --- |
| `src` | Template path inside the payload root |
| `into` | Render target, relative to the deploy target dir |

`into` must normalize to a non-empty relative path — absolute paths,
`..`, root/prefix components, and empty results are
`TemplateError::UnsafeTarget`. `src` follows the same rule inside the
payload (`TemplateError::UnsafeSource`).

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
template.vars  <  auto vars  <  --config file  <  env+args
```

The driver (`tamias-driver-template`) supplies the layers:

1. `template.vars` pack defaults
2. auto vars — `repo` (git `remote.origin.url` basename of the target
   dir, else canonical dir name) and `user` (`$USER`/`$LOGNAME`)
3. `--config` file — JSON by `.json`, TOML by `.toml`, YAML otherwise
   (incl. extensionless); must be a flat map
4. `$TEMPLATE_VAR_<NAME>` env vars, name lowercased, value parsed as
   JSON falling back to string
5. trailing `key=value` args — value parsed as JSON, falling back to
   string

## Errors

| Variant | Meaning |
| --- | --- |
| `Manifest` | Pack manifest or `template:` table failed to parse |
| `UnsafeTarget` | `into` escaped the deploy dir |
| `UnsafeSource` | `src` escaped the payload root |
| `NotUtf8` | Rendered output isn't valid UTF-8 text |
