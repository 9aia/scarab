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
  - icon: '<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915" /> <circle cx="12" cy="12" r="3" /></svg>'
    title: Generic over your config model
    details: ConfigSource<T>/ConfigFormat<T> traits — scarab never names a consumer's Config type; each repo brings its own serde model
    link: /reference/config-port
    linkText: config port
  - icon: '<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10.7 20H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H20a2 2 0 0 1 2 2v4.1" /> <path d="m21 21-1.9-1.9" /> <circle cx="17" cy="17" r="3" /></svg>'
    title: Discovery, not hardcoding
    details: explicit path → $<NAME>_CONFIG → ./<name>.<ext> → ~/.config/<name>/ — YAML preferred on ties
    link: /how-to/customize-config-discovery
    linkText: customize discovery
  - icon: '<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z" /> <path d="M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12" /> <path d="M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17" /></svg>'
    title: Layered configs
    details: LayeredFileSource merges one file per search dir + explicit/override layers as JSON — user config as base, project config on top
    link: /how-to/layered-config
    linkText: layer configs
  - icon: '<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15.39 4.39a1 1 0 0 0 1.68-.474 2.5 2.5 0 1 1 3.014 3.015 1 1 0 0 0-.474 1.68l1.683 1.682a2.414 2.414 0 0 1 0 3.414L19.61 15.39a1 1 0 0 1-1.68-.474 2.5 2.5 0 1 0-3.014 3.015 1 1 0 0 1 .474 1.68l-1.683 1.682a2.414 2.414 0 0 1-3.414 0L8.61 19.61a1 1 0 0 0-1.68.474 2.5 2.5 0 1 1-3.014-3.015 1 1 0 0 0 .474-1.68l-1.683-1.682a2.414 2.414 0 0 1 0-3.414L4.39 8.61a1 1 0 0 1 1.68.474 2.5 2.5 0 1 0 3.014-3.015 1 1 0 0 1-.474-1.68l1.683-1.682a2.414 2.414 0 0 1 3.414 0z" /></svg>'
    title: Template packs via tamias
    details: "type: template packages render declared files into target paths — install validates, deploy writes, uninstall reverses"
    link: /how-to/write-a-template-pack
    linkText: template packs
  - icon: '<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="11" x="3" y="11" rx="2" ry="2" /> <path d="M7 11V7a5 5 0 0 1 10 0v4" /></svg>'
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
