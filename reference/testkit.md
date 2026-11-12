# Testkit

`scarab-testkit` is the behavioral backbone for the config ports:
conformance suites any implementation runs to prove itself, plus
committed golden fixtures. Same pattern as `sepia-testkit` /
`tamias-testkit`. Dev-dependency only — it never ships in a
consumer's binary.

```toml
[dev-dependencies]
scarab-testkit = "0.1"
```

## Format conformance

`assert_config_format_conformance(format, &FormatCase)` checks a
[`ConfigFormat`] against one scenario:

- extensions are claimed, non-empty, and unique
- `parse(text)` produces `expected`
- `render(expected)` re-parses to `expected` (round-trip)
- every `bad_inputs` entry fails with `Err` — never a panic

```rust
use scarab_config_yaml::YamlFormat;
use scarab_testkit::{FormatCase, assert_config_format_conformance};

assert_config_format_conformance(
    &YamlFormat,
    &FormatCase::new("app.yaml", "name: app\nport: 8080\n", cfg)
        .bad(["name: [unclosed"]),
);
```

## Discovery conformance

`assert_discovery_conformance(&case, make)` scripts a whole
filesystem + env scenario into a temp dir — files, search dirs,
env vars (paths resolve against the case root), an optional
explicit path — then runs `load_with_env` on whatever source `make`
builds from the [`PreparedCase`]. The env lookup is injected; the
process environment is never touched.

```rust
use scarab_testkit::{DiscoveryCase, assert_discovery_conformance};

let case = DiscoveryCase::loaded("yaml beats toml", expected)
    .file("myapp.toml", "name = \"toml\"\n")
    .file("myapp.yaml", "name: yaml\n")
    .dir(".");
assert_discovery_conformance(&case, |pc| FileSource::new(
    Discovery::bare("myapp").dirs(pc.dirs().to_vec()),
));
```

`DiscoveryCase::error(name, needle)` expects a failure whose message
contains `needle`. Works for `FileSource`, `LayeredFileSource`, and
any future source implementing [`EnvAwareSource`] (`load_with_env`).

## Source contract

For sources needing no injected env:
`assert_config_source_conformance(source, &expected)` and
`assert_config_source_error(source, "needle")` cover the minimal
[`ConfigSource`] contract.

## Merge conformance

The pure half of layered loading — run once against
`scarab_config::{merge_value, apply_overrides}` themselves, or
against your own merge impl:

- `assert_merge_value_conformance(&[MergeCase])` — objects merge
  key-wise recursively; scalars/arrays/null replace; arrays never
  concatenate
- `assert_apply_overrides_conformance(&[OverrideCase])` — dotted-path
  leaves set in place, intermediate objects materialized

## Template engine conformance

`assert_template_engine_conformance(engine, &[TemplateCase])` is
engine-agnostic — cases carry source + JSON context + expected
output _or_ expected error; only success/failure is contracted,
never error text. `TemplateCase::output` / `::error` build cases;
`assert_template_renders(engine, src, ctx, expected)` is the
one-off assertion.

## Fixtures and the canonical model

- `fixture_path(rel)` / `fixture_text(rel)` — committed golden files
  under `scarab-testkit/fixtures/` (`app.yaml`, `app.toml`,
  `layered/…`)
- `TestConfig` / `ServerConfig` — the canonical model the fixtures
  describe; reuse it, or define your own and reuse the `assert_*`
  suites unchanged

The suites panic on the first violated check — they're test helpers,
not libraries. `scarab-testkit`'s own `tests/conformance.rs` runs
every suite against the workspace impls and is the worked example.
