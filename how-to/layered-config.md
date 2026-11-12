# Layer configs from several files

`FileSource` picks the _first_ matching file — one config wins, the
rest are ignored. `LayeredFileSource` is the merge counterpart: it
collects **one file per search dir**, folds them into a single
`serde_json::Value` tree, applies programmatic overrides, and
deserializes into your model. Use it when a base config (say,
`~/.config/myapp/myapp.yaml`) should be _overridden_ by a project or
environment file rather than replaced by it.

## Swap the source

```rust
use scarab_config_file::LayeredFileSource;

// same Discovery conventions as FileSource::for_app
let cfg: MyConfig = LayeredFileSource::for_app("myapp").load()?;
```

Under `Discovery::for_app` the dirs are `[".", "~/.config/myapp"]`
and the **last dir is the base**: `~/.config/myapp/myapp.yaml`
loads first, `./myapp.yaml` merges over it. Within each dir only the
preferred file counts — `myapp.yaml` beats `myapp.yml` beats
`myapp.toml`, and the losers are not merged.

## Precedence, lowest to highest

1. `<dir>/<name>.<ext>` per search dir, last dir first
2. the explicit path / `$<NAME>_CONFIG` file — merged **on top** of
   every dir layer (unlike `FileSource`, where it replaces discovery)
3. `overrides` — flat dotted-path keys, the highest layer

```rust
use scarab_config_file::LayeredFileSource;
use serde_json::json;
use std::collections::BTreeMap;

let mut src = LayeredFileSource::<MyConfig>::for_app("myapp");
src.explicit = Some("./deploy/prod.yaml".into()); // top file layer
src.overrides = BTreeMap::from([
    ("server.port".to_string(), json!(8443)),
    ("server.tls.enabled".to_string(), json!(true)),
]);
let cfg = src.load()?;
```

## Merge semantics

Layers merge as `serde_json::Value`, never on your typed model —
`MyConfig` only needs `DeserializeOwned` (no `Serialize`).
`merge_value` does the folding:

- **objects merge key-wise, recursively** — a key the overlay omits
  survives from the base
- **scalars, arrays, and null replace** — arrays never concatenate;
  set `[c]` over `[a, b]` and you get `[c]`

Each layer file must parse to a mapping — a top-level array or
scalar is an error naming the path (`config layer is not a
mapping`). Mixed formats across dirs are fine: a TOML user config
under a YAML project config merges cleanly. A parse error names the
file it came from.

## Inspect the layers

`layer_paths()` returns the resolved layer paths in merge order
(lowest precedence first) — handy for logging what will be merged:

```rust
for p in src.layer_paths()? {
    println!("layer: {}", p.display());
}
```

`parse_layer(path)` parses one file into a `serde_json::Value` if
you need to inspect or transform a layer yourself; `load_with(env)`
takes the same injected env lookup as `FileSource` for tests.

## When not to layer

- You want "one file wins, done" — that's `FileSource`; don't pay
  for a merge you don't need.
- Your model can't round-trip through `serde_json::Value` — e.g. it
  deserializes from a non-self-describing format lossily. The merge
  happens on JSON, so exotic serde attributes (`#[serde(with)]`,
  borrowed data) may not survive the intermediate `Value`.
