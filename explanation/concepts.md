# Concepts

The domain vocabulary, each term defined once. Field-level contracts
live in [Reference](../reference/).

## ConfigSource / ConfigFormat

The two config-plane traits. `ConfigSource<T>` finds and loads a `T`
(one `load()`); `ConfigFormat<T>` owns one syntax — serde
parse/render over text, dispatched by file extension. Both are
generic over the consumer's model: scarab never names a `Config`
type. See [Architecture](architecture.md#the-config-plane).

## Discovery

The spec, not the search loop: `env_var`, `dirs`, format preference,
explicit-path handling. `FileSource` interprets it for the
filesystem; other `ConfigSource` impls can reuse the same vocabulary.
`Discovery::for_app(name)` wires the org conventions —
`$<NAME>_CONFIG`, `[".", "~/.config/<name>"]`, YAML preferred.

## Layered loading

`LayeredFileSource` merges one file *per* search dir plus
explicit/override layers — JSON-value merge (objects key-wise,
scalars/arrays replace) before one final deserialize — so user
config is the base and project config lands on top. Contrast
`FileSource`, which takes the *first* match.

## Template pack

A tamias package of `type: template`: a `template:` table in
`tamias.yaml` declaring sources, targets (`into`), variables, and
optional `extends` chains. Packs are ordinary tamias content —
installed, deployed, and uninstalled by `tamias-driver-template`.
See [Reference: template packs](../reference/template-packs).

## TemplateEngine

The render port: `{{ }}` vars + `{% %}` blocks over a context.
`scarab-template-minijinja` is the only v1 impl — sandboxed by
construction, no filesystem or network reach.

## merge_context

Layer precedence for template variables: ancestor defaults → pack
defaults → auto → file → env → args. Defined once in
`scarab-template` so every driver agrees.

## Path safety

`safe_into`/`safe_src`: targets and sources must stay inside the
pack/target roots — `..`, absolute, and empty paths are rejected at
validation, before any write.

## Digest state

The driver's record of what it wrote (sha256 per file). Uninstall is
exact: only files still matching their digest are removed; user
edits survive and are reported.

## Secret names, never values

Config surfaces and template vars carry secret *names* (or
references); [cuora](https://9aia.com/cuora/) owns resolution. This
is a convention the whole org enforces — see
[Boundaries](boundaries.md).
