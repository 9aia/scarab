# Decisions

ADR-lite: the load-bearing decisions, each with the alternative it
beat and the consequence it carries. Design history lives in the
org ledger — proposal P-017 (supersedes P-015).

## Generic over `T`, never a consumer's model

**Decision:** `ConfigSource<T>`/`ConfigFormat<T>` are generic over
the consumer's serde model — scarab never names a `Config` type.

- *Alternative:* a scarab-owned universal config schema.
- *Consequence:* one `FileSource` serves myco, tamias, tarsi, and
every future repo; each consumer keeps only its serde model. Cost:
  trait objects at the call site to keep generics from exploding.

## Discovery is a spec, not a search loop

**Decision:** `Discovery` declares `env_var`, `dirs`, format
preference, explicit-path handling; `FileSource` interprets it.

- *Alternative:* hard-coded per-repo search logic (the
  `gen-*.py`/copy-paste status quo).
- *Consequence:* env/remote `ConfigSource` impls reuse the same
vocabulary without inheriting file semantics.

## Formats are separate crates

**Decision:** `scarab-config-toml` and `-yaml` own only syntax;
`scarab-config` stays pure (traits, `Discovery`, merge helpers).

- *Alternative:* one crate with all formats baked in.
- *Consequence:* a consumer carries just the syntax it wants; the
port stays I/O-free and unit-testable.

## Config and templates are separate planes

**Decision:** config is *read* (find, parse, model); templates are
*write* (render sources to targets with a var context) — two ports,
two invariants.

- *Alternative:* one unified "content" port.
- *Consequence:* a consumer adopts one plane without the other;
discovery order vs. path-safety+digest invariants stay local.

## MiniJinja is the only engine (v1)

**Decision:** `scarab-template-minijinja` is deliberately the sole
`TemplateEngine` impl — sandboxed by construction.

- *Alternative:* a plugin system for template syntax.
- *Consequence:* `{{ }}`/`{% %}` covers config-file templating
without attack surface; the port exists so a second syntax family
can land without touching model or driver.

## Uninstall is digest-exact

**Decision:** the driver records sha256 per written file;
uninstall removes only files still matching.

- *Alternative:* remove everything the pack declared.
- *Consequence:* user edits survive uninstall and are reported —
templated files are owned only until touched.

## Secrets stay names

**Decision:** config surfaces and template vars carry secret
*names*, never values; cuora owns resolution.

- *Alternative:* templated secret injection.
- *Consequence:* packs are safe to distribute and review; the
boundary is convention plus cuora owning the values.
