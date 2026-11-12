---
layout: home

hero:
  name: scarab
  text: config & template expertise
  tagline: generic config loading and templated content packs — the seal that validates what runs
  actions:
    - theme: brand
      text: Getting Started
      link: /getting-started
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
