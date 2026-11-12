# Config port

The config plane is two traits plus a search spec, generic over the
config model `T` (`Deserialize + Serialize`). Scarab never names a
consumer's `Config` type.

## `ConfigFormat<T>`

Syntax-level: which extensions a format claims, `parse`/`render`
over serde. Object-safe via a blanket impl — one `TomlFormat` /
`YamlFormat` value serves every model.

```rust
pub trait ConfigFormat<T>: Send + Sync {
    fn extensions(&self) -> &'static [&'static str];
    fn parse(&self, text: &str) -> Result<T, String>;
    fn render(&self, config: &T) -> Result<String, String>;
}
```

| Impl         | Crate                | Claims                        |
| ------------ | -------------------- | ----------------------------- |
| `YamlFormat` | `scarab-config-yaml` | `yaml`, `yml` (in that order) |
| `TomlFormat` | `scarab-config-toml` | `toml`                        |

`scarab_config::format_for(path, formats)` returns the format
claiming `path`'s extension, or `None`.

## `ConfigSource<T>`

Source-level: `load() -> Result<T, String>`. Impls compose formats —
`scarab-config-file`'s `FileSource` (first-match) and
`LayeredFileSource` (merge-all) are the file-backed impls; env/remote
sources are new impls of the same trait.

## `Discovery`

The shared search spec every file-backed source uses. Nothing is
hardcoded per app — basename, env var, and dirs are caller-supplied.

| Field     | Meaning                                                                               |
| --------- | ------------------------------------------------------------------------------------- |
| `name`    | Config file basename — `myco` → `myco.yaml`, `myco.yml`, `myco.toml`                  |
| `env_var` | Env var naming an explicit config file (`$MYCO_CONFIG`); `None` disables the env step |
| `dirs`    | Directories searched in order for `<name>.<ext>`; `~` expands via `expand_tilde`      |

Constructors and builders:

- `Discovery::for_app(name)` — env var `<NAME>_CONFIG` (uppercased),
  dirs `[".", "~/.config/<name>"]`
- `Discovery::bare(name)` — no env var, no dirs
- `.env_var(var)` / `.without_env()` — set or disable the env step
- `.dirs(iter)` — replace search dirs, in order

## `FileSource<T>`

File-backed `ConfigSource`: discovery + extension dispatch over a set
of `ConfigFormat<T>`s.

| Field       | Meaning                                                       |
| ----------- | ------------------------------------------------------------- |
| `discovery` | Where to search                                               |
| `explicit`  | Explicit `--config`-style path; wins over `discovery.env_var` |
| `formats`   | Syntax formats in preference order                            |

Constructors:

- `FileSource::new(discovery)` — standard formats `[Yaml, Toml]`
- `FileSource::with_formats(discovery, formats)` — caller-chosen set/order
- `FileSource::for_app(name)` — `Discovery::for_app` + standard formats
- `FileSource::standard_formats()` — the `[YamlFormat, TomlFormat]` pair

Requires `T: DeserializeOwned + Serialize` for `new`/`for_app` (the
`ConfigSource` impl bound).

## `LayeredFileSource<T>`

The merge counterpart, in `scarab-config-file`. Where `FileSource`
picks the _first_ existing file, `LayeredFileSource` collects **one
file per search dir** (same per-dir preference rules), folds them
low→high precedence into a `serde_json::Value` tree, applies
programmatic dotted-path overrides, then deserializes into `T`.
Formats are held as `ConfigFormat<serde_json::Value>` — layers merge
as JSON, so `T` only needs `DeserializeOwned` (no `Serialize`).

| Field       | Meaning                                                                                                               |
| ----------- | --------------------------------------------------------------------------------------------------------------------- |
| `discovery` | Where to search — same `Discovery` spec                                                                               |
| `explicit`  | Explicit path — the **top file layer**, merged over all dir layers (unlike `FileSource`, where it replaces discovery) |
| `formats`   | Syntax formats over `serde_json::Value`, in preference order                                                          |
| `overrides` | `BTreeMap<String, Value>` — flat dotted-path keys (`"server.port"`), the highest layer                                |

Constructors mirror `FileSource`: `new(discovery)`, `for_app(name)`,
`with_formats(discovery, formats)`, `standard_formats()`.

Precedence, lowest to highest:

1. `<dir>/<name>.<ext>` per dir, **last dir first** — under
   `Discovery::for_app` (`[".", "~/.config/<name>"]`) the user config
   is the base and `./<name>.yaml` overrides it
2. the explicit path / `$<NAME>_CONFIG` file
3. `overrides` — dotted-path leaf values

Methods: `layer_paths()` / `layer_paths_with(env)` (merge order,
lowest first), `parse_layer(path)` (one file → `Value`; a non-mapping
document is an error), `load()` / `load_with(env)`. Same
`load_with` env-injection seam as `FileSource` for tests. See
[how-to: layered config](/how-to/layered-config) for the merge
semantics and a worked example.

## Merge helpers

Pure functions in `scarab-config` — the half of layered loading that
never touches the filesystem:

- `merge_value(base, overlay)` — recursive JSON merge: objects merge
  key-wise (overlay wins per key, recursing into shared objects);
  scalars, arrays, and null **replace** — arrays never concatenate
- `apply_overrides(base, overrides)` — flat dotted-path keys
  (`"server.port" → 8080`) set leaves, creating intermediate objects;
  a scalar sitting where a path needs to descend is replaced by an
  object. Call after the file layers are merged

## Resolution order

`locate` picks the config path:

1. `explicit` path, else `$<NAME>_CONFIG` — a named path that doesn't
   exist is an error (`config file not found: …`), not a silent skip
2. `<dir>/<name>.<ext>` per search dir, in dir order; within a dir,
   formats in preference order and each format's extensions in
   declared order — YAML before TOML by convention

`load` reads the file and dispatches parsing by extension; an
extensionless explicit path parses with the first (preferred) format;
an unclaimed extension errors with `no config format claims '.<ext>'`.

When nothing is found, the error names the app, the claimed
extensions, and the search steps actually configured.

## Path helpers

- `home_dir()` — `$HOME`, else `$USERPROFILE`, else `/`
- `expand_tilde(path)` — `~`/`~/…` expansion; `~user`, relative, and
  absolute paths pass through untouched
- `user_config_dir(name)` — `~/.config/<name>` (tilde expanded at
  search time)
