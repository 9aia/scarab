# Customize config discovery

`Discovery::for_app` covers the common case. When it doesn't — a
different env var, extra search dirs, a different format set, or an
explicit `--config` path — build the [`FileSource`](/reference/config-port#filesource-t)
directly.

## Custom env var and dirs

```rust
use scarab_config::{Discovery, expand_tilde};
use scarab_config_file::FileSource;

let discovery = Discovery::for_app("myapp")
    .env_var("MYAPP_CFG")                    // override $MYAPP_CONFIG
    .dirs([".", "~/.config/myapp", "/etc/myapp"]);

let src = FileSource::new(discovery);
```

`dirs` entries may contain `~` — expanded via `expand_tilde` at
search time. `.env_var("…")` replaces the env step; `.without_env()`
removes it entirely. `Discovery::bare("myapp")` starts with no env var
and no dirs — fill both yourself.

## Explicit path wins over everything

Set `explicit` on the source (a `--config`-style flag) and it takes
precedence over the env var and all discovery candidates:

```rust
let mut src = FileSource::new(discovery);
src.explicit = Some("/etc/myapp/overrides.toml".into());
```

An explicit path that doesn't exist is an error naming the path — a
named path that misses is a config error, not a silent skip.

## Custom format set or order

`FileSource::new` uses the standard pair — `[YamlFormat,
TomlFormat]`. `with_formats` takes any `Vec<Box<dyn
ConfigFormat<T>>>`; earlier formats win candidate ties within a
directory, and each format's `extensions()` are tried in declared
order. To make TOML preferred:

```rust
use scarab_config_toml::TomlFormat;
use scarab_config_yaml::YamlFormat;

let src = FileSource::with_formats(
    discovery,
    vec![Box::new(TomlFormat), Box::new(YamlFormat)],
);
```

With TOML first, `myapp.toml` beats `myapp.yaml` in the same
directory.

## Extensionless explicit paths

`load_path` dispatches on extension; a path with no extension parses
with the first (preferred) format — the back-compat seam for
`--config` pointing at a suffix-less file. An extension no format
claims is an error: `no config format claims '.json'`.

## Parse a specific file

Skip discovery entirely when the caller already resolved a path:

```rust
let cfg: MyConfig = src.load_path(Path::new("/etc/myapp.toml"))?;
```

## Inject env lookups in tests

`locate_with` / `load_with` take `env: &dyn Fn(&str) ->
Option<String>` so tests never touch the process environment:

```rust
let cfg = src.load_with(&|k| (k == "MYAPP_CONFIG").then(|| "/tmp/x.yaml".into()))?;
```
