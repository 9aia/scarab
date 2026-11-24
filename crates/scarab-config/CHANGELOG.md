# Changelog

## [0.2.0] - 2026-11-24

### Added

- scarab-config loading libs — port + file/toml/yaml impls (P-017 v1) (`a0642ee`)
- **scarab-config:** env_overrides — <PREFIX>_* env layer for config ports (`5892a0e`)
- layered config merge — LayeredFileSource + merge_value (`3a7c52f`)

### Fixed

- **scarab-config:** drop workspace serde_json — preserve_order leaked to consumers (`272154c`)
