# Crates

Generic config + template expertise — ports are traits, format and engine impls are separate crates; consumers plug in their own config model `T` and template pack.

<!-- crate-map -->
| crate | layer | purpose |
|---|---|---|
| `scarab-config` | port | Config port — ConfigSource&lt;T&gt;/ConfigFormat generic over the config model, plus the shared file-discovery spec |
| `scarab-template` | port | Template port — TemplateEngine trait + template-pack model (files, var defaults), generic over the render context |
| `scarab-config-file` | adapter | File impl of ConfigSource — discovery (explicit → env → project → user dir) + extension dispatch to ConfigFormat impls |
| `scarab-config-toml` | adapter | TOML impl of ConfigFormat — parse/render any serde config model |
| `scarab-config-yaml` | adapter | YAML impl of ConfigFormat — parse/render any serde config model |
| `scarab-template-minijinja` | adapter | minijinja impl of scarab_template::TemplateEngine — Jinja-style &#123;&#123; var &#125;&#125; / {% if %} syntax, sandboxed by default |
| `tamias-driver-template` | driver | tamias content driver for type: template — renders template-pack files into declared targets at deploy |
| `scarab-testkit` | testkit | Conformance suites + golden fixtures for the scarab config ports — assert_* functions any ConfigFormat/ConfigSource impl runs to prove itself |
| `scarab-xtask` | tooling | Repo chores: check / test / coverage / install / package / smoke / site |
