# Architecture

Scarab is ports-and-adapters in two planes: **config loading** and
**templated content**. Both are generic — scarab never names a
consumer's types.

## The config plane

```
ConfigSource<T>  ──load()──▶  T            (the port)
      ▲                            ▲
      │ impl                       │ consumer's serde model
scarab-config-file          MyConfig, TamiasConfig, …
      │ dispatches by extension
ConfigFormat<T>  ──parse/render──▶  text   (syntax port)
      ▲
scarab-config-toml / scarab-config-yaml
```

- `scarab-config` is pure: traits, the `Discovery` spec, path
  helpers, and the JSON merge half (`merge_value` /
  `apply_overrides`) — no I/O, no formats, no models.
- `scarab-config-file` is the discovery impl every org repo
  copy-pasted: `FileSource` picks the _first_ matching file;
  `LayeredFileSource` merges _every_ dir's file plus explicit/override
  layers into one `T`. Both dispatch by extension across a format
  list.
- `scarab-config-toml` / `-yaml` own only syntax — serde
  parse/render over any `T`. Formats are separate crates so a
  consumer can carry just the syntax it wants. Layered loading parses
  each layer as `serde_json::Value` through the same blanket impls,
  so `T` itself never needs `Serialize`.

Generic-over-`T` is the load-bearing decision: one `FileSource`
serves `myco`, `tamias`, `tarsi`, and every future repo because the
model stays in the consumer. The trait objects (`&dyn
ConfigFormat<T>`) keep `formats` heterogeneous without generics
exploding at the call site.

Discovery is a _spec_, not a search loop — `FileSource` interprets
it, and env/remote `ConfigSource` impls can reuse the same
vocabulary (`env_var`, `dirs`, preference order) without inheriting
file semantics.

## The template plane

```
tamias pack (type: template)
      │  tamias.yaml `template:` table
      ▼
scarab-template ── PackTemplate/TemplateFile/TemplateEngine (pure)
      │
      ▼
tamias-driver-template ── verbs, vars, state file (the ops impl)
      │
      ▼
scarab-template-minijinja ── the one engine impl (v1)
```

- `scarab-template` holds the pack model and path safety — pure
  domain, engine-agnostic. `merge_context` defines layer precedence;
  `safe_into`/`safe_src` define what "inside the target" means.
  `template.extends` composes packs: tamias materializes each
  ancestor's payload at `.scarab-parent/`, and the driver renders
  the chain ancestor-first so children override parents on `into`
  conflicts — inheritance without a fetch capability in the driver.
- `tamias-driver-template` is where I/O lives: manifest loading,
  chain walking, var resolution (ancestor defaults → pack defaults
  → auto → file → env → args), render+write, digest state. It
  implements the tamias driver contract, so packs distribute through
  tamias like every other content type — no bespoke install path.
- `scarab-template-minijinja` is deliberately the only engine:
  minijinja is small, sandboxed by construction, and its
  `{{ }}`/`{% %}` syntax covers config-file templating without a
  plugin system. The port exists so a second syntax family can land
  without touching the model or the driver.

## Why the split

Config and templates share a shape — _declared inputs in, files
out_ — but not a mechanism. Config is _read_: find a file, parse to
a typed model. Templates are _write_: render declared sources to
declared targets with a variable context. Keeping them as separate
ports means a consumer can adopt one without the other, and each
plane's invariants (discovery order vs. path safety + digest
state) stay local.

## Invariants worth knowing

- **Formats prefer YAML.** Within a directory, `yaml` → `yml` →
  `toml`; across dirs, earlier dirs win regardless of format.
- **Named paths fail loudly.** An explicit/`$*_CONFIG` path that
  misses is an error; discovery candidates skip silently.
- **Templates can't escape.** `..`/absolute/empty targets are
  rejected at validation, before any write.
- **Uninstall is digest-exact.** Only files still matching their
  recorded sha256 are removed; user edits are preserved and
  reported.
- **Layers merge as JSON, not as `T`.** `LayeredFileSource` folds
  `serde_json::Value` trees — objects key-wise, scalars/arrays
  replace — before one final deserialize, so merge semantics stay
  format- and model-agnostic.
- **Secrets stay names.** Config surfaces carry secret _names_,
  never values — cuora owns the values.
