# Scarab

Config & template expertise — generic config loading and templated
content packs. Named for *Scarabaeus sacer* — the sacred scarab
that rolls the ball into place; scarab amulets were seals and
signets — config as the seal that validates what runs. Rust
workspace, ports-and-adapters. `scarab-config` is the port
(`ConfigSource<T>`/`ConfigFormat<T>` traits + `Discovery` spec,
generic over the model — scarab never names a consumer's `Config`
type); `-file`/`-toml`/`-yaml` are the impls.

## Layout

| Crate | Role |
| --- | --- |
| `crates/scarab-config` | config port — traits, `Discovery` spec, `format_for`, tilde/home helpers — pure |
| `crates/scarab-config-file` | file-discovery impl — explicit → `$<NAME>_CONFIG` → `./<name>.<ext>` → `~/.config/<name>/`; YAML preferred |
| `crates/scarab-config-toml` | TOML `ConfigFormat` — serde parse/render |
| `crates/scarab-config-yaml` | YAML `ConfigFormat` — claims `.yaml`/`.yml` |
| `crates/scarab-testkit` | conformance suites (`assert_config_format/discovery/source/merge_*`) + golden fixtures for the config ports |
| `crates/scarab-template` | template port — `TemplateEngine` trait, `PackTemplate` model, `merge_context`, safe-path validation, and the agnostic composition contract (`compose`: `Extends`/`ParentDecl`/`MergeRules` + `compose_chain`/`apply_into_policy` + `PackLoader`/`ParentLocator` ports — spec in `COMPOSITION.md`) |
| `crates/scarab-template-minijinja` | MiniJinja `TemplateEngine` impl — `{{ }}` vars + `{% %}` blocks, sandboxed |
| `crates/tamias-driver-template` | `type: template` content driver — install validates, deploy renders pack payloads (incl. `extends` chains via the materialized-parent locator) to target paths, uninstall removes |
| `xtask` (`scarab-xtask`) | check / test / coverage / install / package / smoke / site |

## Boundary

- Scarab owns **config loading** (and later templated content) —
  consumers keep only their serde `Config` model.
- **tamias** distributes template/config packs; **cuora** owns
  secret values (config carries secret *names*, never values);
  **tarsi** consumes the libs; **hypha** stays pure distributed
  substrate (the P-015 crates were ported here, not kept there).

## Commands

```bash
cargo xtask check          # fmt + clippy -D warnings
cargo xtask test           # whole suite (nextest if present)
cargo xtask coverage       # llvm-cov report; extra args pass through
cargo xtask package        # cargo package --workspace (verify)
cargo xtask site           # build the docs site (needs docs/ worktree)
```

Git hooks live in `.hooks/` (`core.hooksPath` = `.hooks`).

## Rules

- Workspace lints are strict (pedantic + `unwrap_used`/`expect_used`
  warn, `unsafe_code` forbid); `-D warnings` in `xtask check`.
- No `unsafe`. Edition 2024, rust-version 1.85.
- Generic over `T: Deserialize + Serialize` — never name a
  consumer's config type.
- Secret names, never values, in config surfaces.
- Tests `#![allow]` liberally.

## Docs

`docs/` worktree — VitePress + Diátaxis (getting-started, how-to,
reference, explanation) → GitHub Pages at `9aia.com/scarab`.
`cargo xtask site` runs `bun install` + `vitepress build` in the
worktree. Design history: org ledger proposal P-017 (supersedes
P-015).

<!-- myco:agent-awareness -->

## Agent awareness

- Session start / any work discussion: run `myco status --prompt`
  before proposing next steps — know the board first.
- User describes new work → offer to capture it (`myco todo add`,
  `issue new`, `proposal new`). Don't wait to be asked.
- Picking up a task: `myco resume <ID>` is the briefing.
- Wrapping up: `myco handoff <ID>` anything resumable; keep statuses
  honest (claimed → in-progress → blocked/done).

<!-- /myco:agent-awareness -->
