//! [`TemplateEngine`] conformance — engine-agnostic: the suite takes
//! caller-supplied cases (source + JSON context + expected output or
//! expected error), so it works for minijinja today and any other
//! engine dialect tomorrow without assuming a syntax.

use scarab_template::TemplateEngine;
use serde_json::Value;

/// What a [`TemplateCase`] expects `render` to produce.
#[derive(Clone, Debug)]
pub enum TemplateExpect {
    /// Render succeeds and equals this string.
    Output(String),
    /// Render fails (any engine-specific error).
    Error,
}

/// One render scenario against a [`TemplateEngine`].
pub struct TemplateCase {
    /// Scenario name — used in failure messages.
    pub name: String,
    /// Template source text.
    pub source: String,
    /// JSON render context (an object).
    pub ctx: Value,
    /// Expected outcome.
    pub expect: TemplateExpect,
}

impl TemplateCase {
    /// A case expecting `source` rendered against `ctx` to equal
    /// `output`.
    pub fn output(
        name: impl Into<String>,
        source: impl Into<String>,
        ctx: Value,
        output: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            source: source.into(),
            ctx,
            expect: TemplateExpect::Output(output.into()),
        }
    }

    /// A case expecting `source` to fail rendering against `ctx`.
    pub fn error(name: impl Into<String>, source: impl Into<String>, ctx: Value) -> Self {
        Self {
            name: name.into(),
            source: source.into(),
            ctx,
            expect: TemplateExpect::Error,
        }
    }
}

/// Assert `engine` satisfies the [`TemplateEngine`] contract over
/// `cases`: `name()` is non-empty, each case renders to its expected
/// output or fails as expected. Syntax and error *text* are
/// engine-specific by design — only success/failure is contracted.
///
/// # Panics
/// On the first violated case — this is a test helper.
pub fn assert_template_engine_conformance<E>(engine: &E, cases: &[TemplateCase])
where
    E: TemplateEngine + ?Sized,
{
    assert!(
        !engine.name().is_empty(),
        "template engine reports an empty name"
    );
    for case in cases {
        match (engine.render(&case.source, &case.ctx), &case.expect) {
            (Ok(out), TemplateExpect::Output(expected)) => {
                assert_eq!(&out, expected, "template case {}: wrong render", case.name);
            }
            (Ok(out), TemplateExpect::Error) => {
                panic!(
                    "template case {}: expected render error, got {out:?}",
                    case.name
                );
            }
            (Err(e), TemplateExpect::Output(_)) => {
                panic!("template case {}: expected render, got Err: {e}", case.name);
            }
            (Err(_), TemplateExpect::Error) => {}
        }
    }
}

/// Render one-off check for ad-hoc assertions — `render` must
/// succeed and equal `expected`.
///
/// # Panics
/// On an `Err` or mismatch.
pub fn assert_template_renders<E>(engine: &E, source: &str, ctx: &Value, expected: &str)
where
    E: TemplateEngine + ?Sized,
{
    match engine.render(source, ctx) {
        Ok(out) => assert_eq!(&out, expected, "template render mismatch"),
        Err(e) => panic!("template render failed: {e}"),
    }
}
