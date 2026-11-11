//! The canonical config model the committed fixtures describe —
//! field order matters for TOML `render` (scalars/arrays before the
//! `server` table). Downstream repos define their own models; this
//! one exists so every crate exercises the suites against the same
//! shape.

use serde::{Deserialize, Serialize};

/// Top-level test config — mirrors `fixtures/app.{yaml,toml}` and the
/// `fixtures/layered/` merge cases.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TestConfig {
    /// App name.
    pub name: String,
    /// Listen port.
    pub port: u16,
    /// Tag list — arrays *replace* on merge, never concatenate.
    pub tags: Vec<String>,
    /// Nested server block — objects merge key-wise.
    pub server: ServerConfig,
}

/// Nested section of [`TestConfig`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Bind host.
    pub host: String,
    /// TLS enabled.
    pub tls: bool,
}
