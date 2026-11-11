//! Runs the template conformance suite against `MinijinjaEngine`.

use scarab_template_minijinja::MinijinjaEngine;
use scarab_testkit::{TemplateCase, assert_template_engine_conformance, assert_template_renders};
use serde_json::json;

#[test]
fn minijinja_engine_conformance() {
    let engine = MinijinjaEngine::new();
    assert_template_engine_conformance(
        &engine,
        &[
            TemplateCase::output(
                "var-substitution",
                "hello {{ name }}\n",
                json!({"name": "world"}),
                "hello world\n",
            ),
            TemplateCase::output("missing-var-empty", "x{{ missing }}y", json!({}), "xy"),
            TemplateCase::output(
                "loop",
                "{% for i in items %}-{{ i }}{% endfor %}",
                json!({"items": ["a", "b"]}),
                "-a-b",
            ),
            TemplateCase::error("syntax-error", "{% if %}", json!({})),
        ],
    );
}

#[test]
fn minijinja_ad_hoc_render() {
    let engine = MinijinjaEngine::new();
    assert_template_renders(
        &engine,
        "port={{ port }}",
        &json!({"port": 8080}),
        "port=8080",
    );
}
