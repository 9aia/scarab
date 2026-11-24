+++
id = "P-002"
title = "scarab-validate — JSON-Schema validation for untyped documents"
status = "open"
created = "2026-11-24"
owner = "devin"
priority = "med"
+++

# scarab-validate — JSON-Schema validation for untyped documents

## Problem

Scarab validates configs only at the typed edge (`ConfigSource<T>`
deserializes into the consumer's serde model). Untyped documents —
pack manifests, spec files, `config:` mappings — have no validation
leg. The only implementation is trapped: `peregrin-loader/
params_schema.rs` wraps the `jsonschema` crate to validate a
`config:` map against `params.schema.json` (ADR-0081). Mimus
P-001's spec packs will want the same: a spec pack should be able
to ship a schema and self-validate.

## Proposal

New crate `crates/scarab-validate` — the schema-validation leg of
the scarab pipeline (pack → compose → render → **validate** →
typed load):

- `Schema` — compiled `jsonschema::Validator` wrapper:
  `Schema::compile(&Value)` (draft 2020-12 via the `jsonschema`
  crate, same `0.60.x` pin peregrin uses), `validate(&Value) ->
  Vec<Violation>` where `Violation { path, message }` is
  Display-formatted like params_schema's `"{instance_path}:
  {error}"` strings.
- Broken schema = `Err` at compile, never a silent pass (same
  contract as `params_violations`).
- Optional helpers (only if trivial): `Schema::from_file`,
  `validate_file`, convention const for the schema filename —
  keep generic, no `params.schema.json` hardcoding beyond a
  re-exported const consumers may use.
- Format-agnostic: input is `serde_json::Value`; YAML/TOML docs
  convert upstream (scarab-config-* or scarab-compose output).

## Boundary

- scarab never names a consumer's type — validation is over
  `Value`, results are strings/structs the consumer maps to its
  own error type.
- Consumers: peregrin-loader (migrate `params_schema` later),
  mimus spec packs (self-validation), tamias manifest checks,
  any repo shipping JSON-Schema'd config.

## Non-goals

- No custom rule DSL — JSON Schema only.
- No schema *authoring* helpers.
- No migration of peregrin in this change (follow-up).
