//! scarab-template — the template port: the `TemplateEngine` trait
//! plus the template-pack model (`template:` section of a pack's
//! `tamias.yaml`). Pure domain — no I/O, no engine specifics; impls
//! (`scarab-template-minijinja`, …) own the syntax.
//!
//! A **template pack** is a tamias package with `type: template`
//! whose `tamias.yaml` carries a `template:` table:
//!
//! ```toml
//! # tamias.yaml (YAML syntax; the table shape is what matters)
//! name = "myapp-config"
//! type = "template"
//!
//! [template]
//! [[template.files]]
//! src = "app.toml.tmpl"
//! into = "./app.toml"
//! ```
//!
//! `install` validates + previews; `deploy` renders each `src`
//! through the engine with the resolved var context and writes the
//! result at `into` (relative to the target dir — `--target`,
//! `$TEMPLATE_TARGET`, or the caller's cwd). `uninstall` removes
//! files still matching the recorded digests.
//!
//! **Composition (`template.extends`).** A pack may name a parent
//! pack by tamias ref. Resolution is tamias-side — the driver cannot
//! fetch refs — so `tamias install` materializes each parent's
//! payload inside the child payload at `.scarab-parent/` (a parent
//! that itself extends nests as `.scarab-parent/.scarab-parent/`,
//! and so on). The driver renders the chain ancestor-first: parent
//! `template.vars` are the lowest-precedence defaults, and on `into`
//! conflicts the *child* wins (it renders last).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One template file in a pack — `src` inside the payload,
/// `into` the render target (relative path, `..` rejected).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateFile {
    /// Path inside the pack payload (e.g. `app.toml.tmpl`).
    pub src: String,
    /// Render target, relative to the deploy target dir.
    pub into: String,
}

/// The `template:` table of a pack's `tamias.yaml`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PackTemplate {
    /// Files to render, in order.
    pub files: Vec<TemplateFile>,
    /// Variable defaults — lowest-precedence context layer *within
    /// this pack*. Values are arbitrary JSON (strings, numbers,
    /// bools, lists, tables); templates see them as engine-native
    /// values. Across an `extends` chain, ancestor defaults sit below
    /// descendant defaults (child wins).
    pub vars: BTreeMap<String, serde_json::Value>,
    /// `template.extends` — a tamias ref naming a parent template
    /// pack (`git:org/repo//path`, `cf:…`, a store name, …).
    ///
    /// The driver never resolves this itself: tamias materializes the
    /// parent's payload at `.scarab-parent/` inside this payload
    /// during install. `None`/absent = no parent. A non-string or
    /// empty value fails manifest parsing/validation.
    pub extends: Option<String>,
}

/// Errors from pack parsing and path safety — render errors are the
/// engine impl's own error type behind [`TemplateEngine`].
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    /// The pack manifest or `template:` table failed to parse.
    #[error("template manifest: {0}")]
    Manifest(String),
    /// An `into` target escaped the deploy dir (`..`, absolute, or
    /// empty after normalization).
    #[error("unsafe into target {0:?}: must be a relative path inside the target dir")]
    UnsafeTarget(String),
    /// A template `src` escapes the payload root.
    #[error("unsafe src {0:?}: must stay inside the payload")]
    UnsafeSource(String),
    /// Render produced bytes that aren't valid UTF-8 text.
    #[error("rendered output for {0} is not valid UTF-8")]
    NotUtf8(String),
}

/// The template engine port — one impl per syntax family. The
/// context is a JSON object; engines map it to their native value.
pub trait TemplateEngine: Send + Sync {
    /// Engine name for diagnostics (`minijinja`, …).
    fn name(&self) -> &'static str;

    /// Render `source` against `ctx` (a JSON object).
    ///
    /// # Errors
    /// Engine-specific render/parse failure.
    fn render(
        &self,
        source: &str,
        ctx: &serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;
}

/// Merge context layers, later winning: `defaults` < `auto` <
/// `file` < `args`. All layers are flat `name → JSON value` maps;
/// values may themselves be objects/arrays for nested access.
#[must_use]
pub fn merge_context(
    defaults: &BTreeMap<String, serde_json::Value>,
    auto: &BTreeMap<String, serde_json::Value>,
    file: &BTreeMap<String, serde_json::Value>,
    args: &BTreeMap<String, serde_json::Value>,
) -> serde_json::Value {
    let mut ctx = serde_json::Map::new();
    for layer in [defaults, auto, file, args] {
        for (k, v) in layer {
            ctx.insert(k.clone(), v.clone());
        }
    }
    serde_json::Value::Object(ctx)
}

/// Validate an `into` target: relative, no `..`, no root/prefix —
/// returns the normalized relative [`PathBuf`] or
/// [`TemplateError::UnsafeTarget`].
///
/// # Errors
/// [`TemplateError::UnsafeTarget`] on escape attempts.
pub fn safe_into(into: &str) -> Result<PathBuf, TemplateError> {
    let p = PathBuf::from(into);
    if p.is_absolute() || into.trim().is_empty() {
        return Err(TemplateError::UnsafeTarget(into.to_string()));
    }
    let mut norm = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::Normal(c) => norm.push(c),
            std::path::Component::CurDir => {}
            _ => return Err(TemplateError::UnsafeTarget(into.to_string())),
        }
    }
    if norm.as_os_str().is_empty() {
        return Err(TemplateError::UnsafeTarget(into.to_string()));
    }
    Ok(norm)
}

/// Validate a template `src`: a relative path that stays inside the
/// payload (no `..`, no absolute).
///
/// # Errors
/// [`TemplateError::UnsafeSource`] on escape attempts.
pub fn safe_src(src: &str) -> Result<PathBuf, TemplateError> {
    safe_into(src).map_err(|_| TemplateError::UnsafeSource(src.to_string()))
}
