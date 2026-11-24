+++
id = "P-001"
title = "scarab-compose — document-level composition kernel"
status = "open"
created = "2026-11-24"
owner = "devin"
priority = "med"
+++

# scarab-compose — document-level composition kernel

## Problem

Org needs document-level composition (`include:`/`extends:`/
`deep_merge` over untyped value trees + `tamias:` refs) for
standard/spec packs (see mimus P-001). The machinery exists but is
trapped: `peregrin-loader-yaml`'s `expand.rs`+`tamias.rs` (~850 ln,
serde_json::Value-generic, no campaign types) solves exactly this —
campaign-flavored naming aside. `scarab-template`'s COMPOSITION.md
already specifies the pack-level contract (`extends`, `merge`,
ancestor order, diamond collapse, `PackLoader`/`ParentLocator`
ports) and tamias-install already materializes parent chains —
but nothing owns *document-internal* composition.

## Proposal

New crate `crates/scarab-compose`: the generic kernel, re-homed
(not extracted blindly — cleaned up):

- `deep_merge` (local wins, dicts per-key, arrays concat)
- `include:` dict key + `${include(path)}` string expansion,
  circular-include guards
- `extends:` chains per COMPOSITION.md semantics (ancestor-first,
  diamond collapse, `merge.*` rules) at document level
- `snippet:` machinery only if a consumer needs it (peregrin does;
  keep behind a feature or port lazily)
- `tamias:<spec>//<path>` resolution — port of
  `peregrin-loader-yaml/src/tamias.rs` (store-first, fetch-on-miss
  via tamias-install)
- generic error type; consumers deserialize the resolved `Value`
  into their own model (mimus spec, campaign, policy doc)

Format-agnostic: YAML/TOML/JSON all parse to `Value` upstream;
scarab-config-{toml,yaml} already exist for the parse leg.

## Consumers

- **mimus** standard tier — spec doc resolution (P-001)
- **peregrin-loader-yaml** — later migration: its expand.rs is the
  same machinery; keeps only auto_discover + Campaign::from_dict
- **lasius** policy / **cypera** profiles — future

## Non-goals

- No typed models — scarab never names a consumer's type (existing
  boundary rule).
- No fetch semantics changes — tamias-install stays the fetch
  pipeline.
- No campaign `nodes:` dedupe — that's peregrin's model concern.

## Sibling candidate (noted, not scoped)

`peregrin-loader/params_schema.rs` — JSON-Schema validation of
untyped config mappings — is the same trap pattern. If spec packs
need self-validation, a `scarab-validate` crate is the natural
third leg; evaluate after compose lands.
