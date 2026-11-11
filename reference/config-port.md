---
url: /scarab/reference/config-port.md
---
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

| Impl | Crate | Claims |
| --- | --- | --- |
| `YamlFormat` | `scarab-config-yaml` | `yaml`, `yml` (in that order) |
| `TomlFormat` | `scarab-config-toml` | `toml` |

`scarab_config::format_for(path, formats)` returns the format
claiming `path`'s extension, or `None`.

## `ConfigSource<T>`

Source-level: `load() -> Result<T, String>`. Impls compose formats —
`scarab-config-file`'s `FileSource` is the discovery + dispatch impl;
env/remote sources are new impls of the same trait.

## `Discovery`

The shared search spec every file-backed source uses. Nothing is
hardcoded per app — basename, env var, and dirs are caller-supplied.

| Field | Meaning |
| --- | --- |
| `name` | Config file basename — `myco` → `myco.yaml`, `myco.yml`, `myco.toml` |
| `env_var` | Env var naming an explicit config file (`$MYCO_CONFIG`); `None` disables the env step |
| `dirs` | Directories searched in order for `<name>.<ext>`; `~` expands via `expand_tilde` |

Constructors and builders:

* `Discovery::for_app(name)` — env var `<NAME>_CONFIG` (uppercased),
  dirs `[".", "~/.config/<name>"]`
* `Discovery::bare(name)` — no env var, no dirs
* `.env_var(var)` / `.without_env()` — set or disable the env step
* `.dirs(iter)` — replace search dirs, in order

## `FileSource<T>`

File-backed `ConfigSource`: discovery + extension dispatch over a set
of `ConfigFormat<T>`s.

| Field | Meaning |
| --- | --- |
| `discovery` | Where to search |
| `explicit` | Explicit `--config`-style path; wins over `discovery.env_var` |
| `formats` | Syntax formats in preference order |

Constructors:

* `FileSource::new(discovery)` — standard formats `[Yaml, Toml]`
* `FileSource::with_formats(discovery, formats)` — caller-chosen set/order
* `FileSource::for_app(name)` — `Discovery::for_app` + standard formats
* `FileSource::standard_formats()` — the `[YamlFormat, TomlFormat]` pair

Requires `T: DeserializeOwned + Serialize` for `new`/`for_app` (the
`ConfigSource` impl bound).

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

* `home_dir()` — `$HOME`, else `$USERPROFILE`, else `/`
* `expand_tilde(path)` — `~`/`~/…` expansion; `~user`, relative, and
  absolute paths pass through untouched
* `user_config_dir(name)` — `~/.config/<name>` (tilde expanded at
  search time)
