//! scarab-template-minijinja — the minijinja [`TemplateEngine`]
//! impl: `{{ var }}`/`{% if %}`/`{% for %}` syntax, sandboxed
//! (no filesystem/HTTP access from templates), undefined vars
//! render as empty by default.
//!
//! Chosen over tera/handlebars for v1: single small dep tree,
//! sandboxed by construction (pack templates are third-party
//! content — they must not reach the filesystem), and the syntax
//! covers config-file templating without a plugin system. Recorded
//! in the repo ADR.

use scarab_template::TemplateEngine;

/// The minijinja engine — a configured [`minijinja::Environment`].
#[derive(Debug, Clone)]
pub struct MinijinjaEngine {
    env: minijinja::Environment<'static>,
}

impl Default for MinijinjaEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MinijinjaEngine {
    /// A fresh environment with default settings.
    #[must_use]
    pub fn new() -> Self {
        let mut env = minijinja::Environment::new();
        // Config files should keep their trailing newline — the
        // engine default strips one, which concatenates files on
        // `cat` and trips POSIX-text linters.
        env.set_keep_trailing_newline(true);
        Self { env }
    }
}

impl TemplateEngine for MinijinjaEngine {
    fn name(&self) -> &'static str {
        "minijinja"
    }

    fn render(
        &self,
        source: &str,
        ctx: &serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let tmpl = self.env.template_from_str(source)?;
        let value = minijinja::Value::from_serialize(ctx);
        Ok(tmpl.render(value)?)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_vars_and_conditionals() {
        let engine = MinijinjaEngine::new();
        let ctx = json!({"name": "world", "prod": true, "items": ["a", "b"]});
        let out = engine
            .render(
                "hello {{ name }}{% if prod %} PRODUCTION{% endif %}\n{% for i in items %}-{{ i }}\n{% endfor %}",
                &ctx,
            )
            .unwrap();
        assert_eq!(out, "hello world PRODUCTION\n-a\n-b\n");
    }

    #[test]
    fn undefined_var_renders_empty() {
        let engine = MinijinjaEngine::new();
        let out = engine.render("x{{ missing }}y", &json!({})).unwrap();
        assert_eq!(out, "xy");
    }

    #[test]
    fn syntax_error_is_error() {
        let engine = MinijinjaEngine::new();
        assert!(engine.render("{% if %}", &json!({})).is_err());
    }
}
