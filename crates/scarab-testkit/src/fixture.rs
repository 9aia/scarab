//! Golden-fixture access — committed files under
//! `crates/scarab-testkit/fixtures/`, resolved via `CARGO_MANIFEST_DIR`
//! so they work from any crate that dev-depends on this one.

use std::path::{Path, PathBuf};

/// Absolute path of fixture `rel` (e.g. `"app.yaml"`,
/// `"layered/user/myapp.yaml"`).
#[must_use]
pub fn fixture_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(rel)
}

/// Contents of fixture `rel` as a `String`.
///
/// # Panics
/// On a missing/unreadable fixture — always a fixture bug.
#[must_use]
pub fn fixture_text(rel: &str) -> String {
    let path = fixture_path(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()))
}
