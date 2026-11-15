---
layout: home

hero:
  name: scarab
  text: config & template expertise
  tagline: generic config loading and templated content packs — the seal that validates what runs
  actions:
    - theme: brand
      text: Getting Started
      link: /tutorials/getting-started
    - theme: alt
      text: Architecture
      link: /explanation/architecture

features:
  - icon: ⚙️
    title: Generic over your config model
    details: ConfigSource<T>/ConfigFormat<T> traits — scarab never names a consumer's Config type; each repo brings its own serde model
    link: /reference/config-port
    linkText: config port
  - icon: 🗂️
    title: Discovery, not hardcoding
    details: explicit path → $<NAME>_CONFIG → ./<name>.<ext> → ~/.config/<name>/ — YAML preferred on ties
    link: /how-to/customize-config-discovery
    linkText: customize discovery
  - icon: 🧱
    title: Layered configs
    details: LayeredFileSource merges one file per search dir + explicit/override layers as JSON — user config as base, project config on top
    link: /how-to/layered-config
    linkText: layer configs
  - icon: 🧩
    title: Template packs via tamias
    details: "type: template packages render declared files into target paths — install validates, deploy writes, uninstall reverses"
    link: /how-to/write-a-template-pack
    linkText: template packs
  - icon: 🔒
    title: Sandboxed rendering
    details: minijinja engine — templates are third-party content and cannot reach the filesystem or network
    link: /explanation/architecture
    linkText: architecture
---

## What it is

Scarab is the org's config & template expertise: a generic config
loading port (`ConfigSource<T>`/`ConfigFormat<T>` + a `Discovery`
spec, generic over the consumer's serde model) and a templated
content plane (`type: template` tamias packs rendered by a
sandboxed MiniJinja engine).

## Why

Every repo was copy-pasting the same config-discovery dance —
explicit path → `$<NAME>_CONFIG` → `./<name>.<ext>` →
`~/.config/<name>/` — and templated dotfiles/config packs had no
shared mechanism. Scarab generalizes both once, behind ports, so
consumers keep only their `Config` type.

## Boundary

- **Owns:** config discovery + formats, the `type: template` pack
  model/driver/engine.
- **Doesn't own:** secret values ([cuora](https://9aia.com/cuora/)),
  distribution ([tamias](https://9aia.com/tamias/)), releases
  ([tarsi](https://9aia.com/tarsi/)).

## Where to go next

| You want to… | Go to |
| --- | --- |
| Load your first typed config | [Tutorial](/tutorials/getting-started) |
| Do a task (custom discovery, layered configs, template packs) | [How-to](/how-to/) |
| Understand the design (ports, planes, boundaries) | [Explanation](/explanation/) |
| Look up a trait, type, or pack field | [Reference](/reference/) |
