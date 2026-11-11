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
  helpers — no I/O, no formats, no models.
- `scarab-config-file` is the discovery impl every org repo
  copy-pasted: explicit path → env var → search dirs, extension
  dispatch across a format list.
- `scarab-config-toml` / `-yaml` own only syntax — serde
  parse/render over any `T`. Formats are separate crates so a
  consumer can carry just the syntax it wants.

Generic-over-`T` is the load-bearing decision: one `FileSource`
serves `myco`, `tamias`, `tarsi`, and every future repo because the
model stays in the consumer. The trait objects (`&dyn
ConfigFormat<T>`) keep `formats` heterogeneous without generics
exploding at the call site.

Discovery is a *spec*, not a search loop — `FileSource` interprets
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
- `tamias-driver-template` is where I/O lives: manifest loading,
  var resolution (pack defaults → auto → file → env → args),
  render+write, digest state. It implements the tamias driver
  contract, so packs distribute through tamias like every other
  content type — no bespoke install path.
- `scarab-template-minijinja` is deliberately the only engine:
  minijinja is small, sandboxed by construction, and its
  `{{ }}`/`{% %}` syntax covers config-file templating without a
  plugin system. The port exists so a second syntax family can land
  without touching the model or the driver.

## Why the split

Config and templates share a shape — *declared inputs in, files
out* — but not a mechanism. Config is *read*: find a file, parse to
a typed model. Templates are *write*: render declared sources to
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
- **Secrets stay names.** Config surfaces carry secret *names*,
  never values — cuora owns the values.
