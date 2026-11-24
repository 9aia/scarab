+++
id = "PL-002"
title = "scarab-validate implementation"
status = "open"
created = "2026-11-24"
owner = "devin"
priority = "med"
+++

# Plan — scarab-validate implementation

Implements scarab P-002. Ports `peregrin-loader/src/params_schema.rs`
into a generic crate.

## Phase 1 — scaffold

- `crates/scarab-validate/Cargo.toml` — workspace member; deps:
  `serde_json` (workspace), `jsonschema = "0.60.2"` (same version
  peregrin pins — check root workspace Cargo.toml; if jsonschema
  is a workspace dep reuse it, else add a plain version dep in
  house style).
- `src/lib.rs` — crate docs + public API.

## Phase 2 — port + generalize

From `repos/peregrin/crates/peregrin-loader/src/params_schema.rs`:

- `params_violations(schema, config_map)` → `Schema::compile` +
  `Schema::validate(&Value) -> Vec<Violation>`.
- `Violation` struct: `path: String` (instance_path, empty for
  root), `message: String`; `Display` = `"{path}: {message}"` or
  bare message when path empty — preserve the exact formatting
  peregrin tests assert (`"timout"` substring, `"token_env"`,
  `"minimum"`, `"does not match"`).
- `Schema::compile` returns `Result<Schema, String>` — broken
  schema is an error, never skipped.
- Keep `PARAMS_SCHEMA_FILE` name available as a const consumers
  can re-export (call it `SCHEMA_FILE_HINT` or keep the name —
  pick what reads generically; document it's a convention, not
  enforced).
- Compile-once semantics: `jsonschema::validator_for` is the
  expensive step; `Schema` holds the compiled validator and
  `validate` is cheap — document this (it's why the type exists
  vs a free function).

## Phase 3 — tests

Port all 5 tests from params_schema.rs verbatim-ish into
`src/lib.rs` `#[cfg(test)]` or `tests/`:

- valid config → no violations
- additionalProperties:false typo key → 1 violation containing key
- missing required → violation containing field name
- wrong type/bounds → violations ("minimum", pattern "does not
  match")
- unusable schema → compile Err

Plus: `Display` formatting check (path-prefixed vs root).

## Phase 4 — verify

- `cargo build -p scarab-validate`, `cargo test -p scarab-validate`
- `cargo clippy -p scarab-validate --all-targets -- -D warnings`
- `cargo fmt` new files
- Register in workspace members.

## Out of scope

- peregrin params_schema migration (follow-up task)
- YAML/TOML schema loading helpers (consumers convert to Value)
- Custom validation rules beyond JSON Schema
