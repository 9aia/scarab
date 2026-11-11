//! scarab-testkit — the behavioral backbone for the scarab config
//! ports: conformance suites any implementation runs to prove itself,
//! plus golden fixtures under `fixtures/`. Same pattern as
//! `sepia-testkit` / `tamias-testkit`.
//!
//! - [`assert_config_format_conformance`] — a [`ConfigFormat`] must
//!   claim non-empty unique extensions, parse a known-good document
//!   to the expected model, round-trip `render` → `parse`, and fail
//!   with `Err` (never panic) on malformed input.
//! - [`assert_discovery_conformance`] — a scripted file/env scenario
//!   (written into a temp dir, env lookups injected — never the
//!   process environment) run against any [`EnvAwareSource`]:
//!   `FileSource`, `LayeredFileSource`, or a future env/remote source
//!   that accepts an injected lookup.
//! - [`assert_config_source_conformance`] /
//!   [`assert_config_source_error`] — the minimal
//!   [`ConfigSource`] contract for sources that need no injected env.
//! - [`assert_merge_value_conformance`] /
//!   [`assert_apply_overrides_conformance`] — the pure JSON merge
//!   contract layered sources build on (`merge_value` /
//!   `apply_overrides`).
//! - [`assert_template_engine_conformance`] — engine-agnostic
//!   render cases for any [`TemplateEngine`] impl.
//! - [`fixture_path`] / [`fixture_text`] — committed golden files.
//!
//! [`TestConfig`] is the canonical model the committed fixtures
//! describe; downstream repos define their own and reuse the
//! `assert_*` suites unchanged.
//!
//! [`ConfigFormat`]: scarab_config::ConfigFormat
//! [`ConfigSource`]: scarab_config::ConfigSource

pub mod fixture;
pub mod format;
pub mod merge;
pub mod model;
pub mod source;
pub mod template;

pub use fixture::{fixture_path, fixture_text};
pub use format::{FormatCase, assert_config_format_conformance};
pub use merge::{
    MergeCase, OverrideCase, assert_apply_overrides_conformance, assert_merge_value_conformance,
};
pub use model::{ServerConfig, TestConfig};
pub use source::{
    DiscoveryCase, EnvAwareSource, EnvValue, Expect, PreparedCase,
    assert_config_source_conformance, assert_config_source_error, assert_discovery_conformance,
};
pub use template::{
    TemplateCase, TemplateExpect, assert_template_engine_conformance, assert_template_renders,
};
