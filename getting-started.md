# Getting started

Load a typed config from a file with `scarab-config-file`. The whole
flow is three steps: define your serde model, pick a
[`Discovery`](/reference/config-port#discovery) spec, load.

## 1. Add the crates

```toml
[dependencies]
scarab-config = "0.1"
scarab-config-file = "0.1"
scarab-config-yaml = "0.1"
scarab-config-toml = "0.1"
serde = { version = "1", features = ["derive"] }
```

## 2. Define your config model

Scarab stays generic — your repo owns the `Config` type:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct MyConfig {
    name: String,
    port: u16,
}
```

## 3. Load it

```rust
use scarab_config_file::FileSource;

let cfg: MyConfig = FileSource::for_app("myapp").load()?;
```

`FileSource::for_app("myapp")` wires the org conventions for you:
env var `MYAPP_CONFIG`, search dirs `[".", "~/.config/myapp"]`, and
the standard format pair (YAML preferred, TOML fallback). For `myapp`
it resolves, in order:

1. `$MYAPP_CONFIG` — an explicit file path
2. `./myapp.yaml`, `./myapp.yml`, `./myapp.toml`
3. `~/.config/myapp/myapp.yaml`, `.yml`, `.toml`

The first existing file wins. See [reference: config
port](/reference/config-port) for the full contract and [how-to:
customize discovery](/how-to/customize-config-discovery) for custom
search specs.

## Next steps

- [Write a template pack](/how-to/write-a-template-pack) — ship
  rendered config files as a tamias `type: template` package.
- [Deploy a template pack](/how-to/deploy-a-template-pack) — the
  driver verbs, variable layers, and uninstall semantics.
