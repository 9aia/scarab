+++
id = "PL-001"
title = "scarab-compose implementation"
status = "open"
created = "2026-11-24"
owner = "devin"
priority = "med"
links = ["P-001"]
+++

# Plan — scarab-compose implementation

Implements scarab P-001. Ports the document-composition kernel out
of `peregrin-loader-yaml` into a new `crates/scarab-compose`.

## Phase 1 — scaffold

- `crates/scarab-compose/Cargo.toml` — workspace member; deps:
  `serde_json`, `serde_yaml_ng` (workspace), `thiserror`,
  `tamias-core`/`tamias-store-local`/`tamias-install` git deps at
  rev `255f44cbb530fd11a38b4fee5356524bf927e402` (same pin as
  peregrin), behind a default `tamias` feature so minimal installs
  skip the fetch pipeline.
- `src/lib.rs` — crate docs, module wiring, public re-exports.
- Register in workspace `Cargo.toml` members + root lints already
  apply (pedantic, `unwrap_used`/`expect_used` warn, `unsafe_code`
  forbid — tests `#![allow]` liberally per house style).

## Phase 2 — port the kernel (from repos/peregrin)

Source: `repos/peregrin/crates/peregrin-loader-yaml/src/`

- `expand.rs` → `src/expand.rs` — `deep_merge`,
  `resolve_include_dict`, `resolve_include_strings`,
  `resolve_snippet_{definitions,refs,strings}`, `normalize`,
  path helpers. De-peregrinize: `peregrin_core::js_string` → local
  copy (~15 ln); `peregrin_loader::LoadError` → `ComposeError`.
  `dedupe_node_ids`/`merge_documents` stay OUT (campaign model).
- `tamias.rs` → `src/tamias.rs` — verbatim port minus
  peregrin types; `TamiasRoots` becomes public API.
- `parse.rs` → `src/parse.rs` — `parse_yaml`/`parse_json`/
  `read_text_file` helpers (pub(crate)).
- Keep circular-include `seen` guards and error messages stable —
  tests assert on them.

## Phase 3 — public API

```rust
pub struct Composer { /* TamiasRoots opt */ }
impl Composer {
    pub fn resolve_yaml(&self, text: &str, base_dir: &Path) -> Result<Value, ComposeError>;
    pub fn resolve_toml(&self, text: &str, base_dir: &Path) -> Result<Value, ComposeError>;
    pub fn resolve_json(&self, text: &str, base_dir: &Path) -> Result<Value, ComposeError>;
    pub fn resolve_file(&self, path: &Path) -> Result<Value, ComposeError>;
}
// pipeline order = peregrin's load_raw_inner minus campaign steps:
//   parse → include-dict → snippet-defs → snippet-refs →
//   snippet-strings → include-strings
```

`extends:` doc-level chains = phase 4 (follow-up; COMPOSITION.md
semantics, ancestor-first, diamond collapse) — scaffold the
`compose` module signature now, implement next iteration.

## Phase 4 — tests

Port the generic cases from
`repos/peregrin/crates/peregrin-loader-yaml/tests/loader.rs`
(983 ln) into `crates/scarab-compose/tests/compose.rs`, asserting
on resolved `Value` instead of `Campaign`:

- include dict merge, local wins, include lists order
- `${include()}` whole + partial + nested expressions
- recursive includes, circular detection (key + expression + self)
- missing file / non-string path errors
- snippet defs inheritance, refs, `${snippet()}` strings
- tamias: ref resolution against a fixture store (skip fetch —
  store-only `TamiasRoots`)

## Phase 5 — verify + docs

- `cargo xtask check` (fmt + clippy -D warnings)
- `cargo test -p scarab-compose` (+ `cargo xtask test` if cheap)
- crate-level docs explain the contract pointer →
  `scarab-template/COMPOSITION.md`; note peregrin migration as
  follow-up (separate change, not this PR-equivalent)

## Out of scope

- peregrin-loader-yaml migration (follow-up proposal/task)
- `extends:` implementation (phase 4 of this plan = next iter)
- any consumer wiring (mimus P-001 is downstream)
