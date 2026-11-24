---
url: /scarab/reference/crates.md
---
# Crates

Generic config + template expertise — ports are traits, format and engine impls are separate crates; consumers plug in their own config model `T` and template pack.

| crate | layer | purpose |
|---|---|---|
| `scarab-config` | port | Config port — ConfigSource\<T>/ConfigFormat generic over the config model, plus the shared file-discovery spec |
| `scarab-template` | port | Template port — TemplateEngine trait + template-pack model (files, var defaults), generic over the render context |
| `scarab-config-file` | adapter | File impl of ConfigSource — discovery (explicit → env → project → user dir) + extension dispatch to ConfigFormat impls |
| `scarab-config-toml` | adapter | TOML impl of ConfigFormat — parse/render any serde config model |
| `scarab-config-yaml` | adapter | YAML impl of ConfigFormat — parse/render any serde config model |
| `scarab-template-minijinja` | adapter | minijinja impl of scarab_template::TemplateEngine — Jinja-style {{ var }} / {% if %} syntax, sandboxed by default |
| `scarab-compose` | engine | Document composition — include/snippet expansion over YAML/TOML/JSON trees into serde_json::Value |
| `scarab-validate` | engine | JSON Schema validation over serde_json::Value — compile once, validate many |
| `tamias-driver-template` | driver | tamias content driver for type: template — renders template-pack files into declared targets at deploy |
| `scarab-testkit` | testkit | Conformance suites + golden fixtures for the scarab config ports — assert\_\* functions any ConfigFormat/ConfigSource impl runs to prove itself |
| `scarab-xtask` | tooling | Repo chores: check / test / coverage / install / package / smoke / site |
