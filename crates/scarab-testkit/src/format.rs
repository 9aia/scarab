//! [`ConfigFormat`] conformance — syntax-level contract every format
//! impl (toml, yaml, json, …) must satisfy for every config model `T`.

use std::fmt::Debug;

use scarab_config::ConfigFormat;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// One format conformance scenario: `text` must parse to `expected`,
/// `expected` must render and re-parse to itself, and every
/// `bad_inputs` entry must be an `Err` (never a panic).
pub struct FormatCase<T> {
    /// Scenario name — used in failure messages.
    pub name: String,
    /// Document text the format must parse.
    pub text: String,
    /// The model `text` must produce.
    pub expected: T,
    /// Inputs that must fail with `Err`, not panic.
    pub bad_inputs: Vec<String>,
}

impl<T> FormatCase<T> {
    /// A case with no bad inputs — chain [`bad`](Self::bad) to add.
    pub fn new(name: impl Into<String>, text: impl Into<String>, expected: T) -> Self {
        Self {
            name: name.into(),
            text: text.into(),
            expected,
            bad_inputs: Vec::new(),
        }
    }

    /// Add inputs that must fail parsing.
    #[must_use]
    pub fn bad(mut self, inputs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.bad_inputs.extend(inputs.into_iter().map(Into::into));
        self
    }
}

/// Assert `format` satisfies the [`ConfigFormat`] contract for `case`.
///
/// Checks, in order: extensions are claimed, non-empty, and unique;
/// `parse(text) == expected`; `render(expected)` succeeds and
/// `parse(render(expected)) == expected`; every bad input is `Err`.
///
/// # Panics
/// On the first violated check — this is a test helper.
pub fn assert_config_format_conformance<T, F>(format: &F, case: &FormatCase<T>)
where
    T: DeserializeOwned + Serialize + PartialEq + Debug,
    F: ConfigFormat<T> + ?Sized,
{
    let exts = format.extensions();
    assert!(
        !exts.is_empty(),
        "case {}: no extensions claimed",
        case.name
    );
    for ext in exts {
        assert!(
            !ext.is_empty(),
            "case {}: empty extension claimed",
            case.name
        );
    }
    let mut seen = std::collections::BTreeSet::new();
    for ext in exts {
        assert!(
            seen.insert(*ext),
            "case {}: duplicate extension '{ext}'",
            case.name
        );
    }

    let parsed = format
        .parse(&case.text)
        .unwrap_or_else(|e| panic!("case {}: parse failed: {e}", case.name));
    assert_eq!(
        parsed, case.expected,
        "case {}: parse produced wrong model",
        case.name
    );

    let rendered = format
        .render(&case.expected)
        .unwrap_or_else(|e| panic!("case {}: render failed: {e}", case.name));
    let reparsed = format
        .parse(&rendered)
        .unwrap_or_else(|e| panic!("case {}: render→parse failed: {e}", case.name));
    assert_eq!(
        reparsed, case.expected,
        "case {}: render→parse round-trip mismatch",
        case.name
    );

    for bad in &case.bad_inputs {
        match format.parse(bad) {
            Err(_) => {}
            Ok(v) => panic!(
                "case {}: bad input parsed successfully: {bad:?} → {v:?}",
                case.name
            ),
        }
    }
}
