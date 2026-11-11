//! Merge conformance — the pure `serde_json::Value` contract that
//! layered config sources (`LayeredFileSource`, future remote/env
//! merges) build on: [`scarab_config::merge_value`] and
//! [`scarab_config::apply_overrides`].

use std::collections::BTreeMap;

use scarab_config::{apply_overrides, merge_value};
use serde_json::Value;

/// One `merge_value` scenario: `overlays` fold low→high onto `base`,
/// producing `expected`.
pub struct MergeCase {
    /// Scenario name — used in failure messages.
    pub name: String,
    /// Starting tree.
    pub base: Value,
    /// Overlays applied in order (later = higher precedence).
    pub overlays: Vec<Value>,
    /// The merged result.
    pub expected: Value,
}

/// Assert the merge contract over `cases`: objects merge key-wise
/// recursively, scalars/arrays/null replace, arrays never
/// concatenate.
///
/// # Panics
/// On the first violated case — this is a test helper.
pub fn assert_merge_value_conformance(cases: &[MergeCase]) {
    for case in cases {
        let mut base = case.base.clone();
        for overlay in &case.overlays {
            merge_value(&mut base, overlay.clone());
        }
        assert_eq!(
            base, case.expected,
            "merge case {}: wrong result",
            case.name
        );
    }
}

/// One `apply_overrides` scenario: flat dotted-path `overrides` apply
/// onto `base`, producing `expected`.
pub struct OverrideCase {
    /// Scenario name — used in failure messages.
    pub name: String,
    /// Starting tree.
    pub base: Value,
    /// Dotted-path leaf overrides.
    pub overrides: BTreeMap<String, Value>,
    /// The resulting tree.
    pub expected: Value,
}

/// Assert the dotted-path override contract: leaves set in place,
/// intermediate objects materialized, scalars in the way replaced.
///
/// # Panics
/// On the first violated case — this is a test helper.
pub fn assert_apply_overrides_conformance(cases: &[OverrideCase]) {
    for case in cases {
        let mut base = case.base.clone();
        apply_overrides(&mut base, &case.overrides);
        assert_eq!(
            base, case.expected,
            "override case {}: wrong result",
            case.name
        );
    }
}
