//! scarab-compose — document composition: `include:`/`snippet:`
//! expansion and `${include(...)}`/`${snippet(...)}` expressions over
//! untyped [`serde_json::Value`] trees.
//!
//! This is the generic half of peregrin's `peregrin-loader-yaml`
//! pipeline (itself a port of the TS `loader.ts`), minus every
//! campaign-model concern: no discovery, no typed `Campaign`, no node
//! id dedupe, no `params.schema.json` validation. Parse → expand →
//! resolved `Value`.
//!
//! Pipeline (matching the TS `loadRaw`, in order):
//! 0. `extends:` chain resolution — document-level parent composition
//!    (COMPOSITION.md semantics: ancestor-first, diamonds collapse,
//!    cycles/depth cap error; see `src/extends.rs`). Each body gets
//!    its `include:`/`${include(...)}` passes
//!    anchored at its own directory before the ancestor-first fold;
//!    the snippet passes then run on the merged document.
//! 1. parse the document (YAML / TOML / JSON — all land on
//!    [`serde_json::Value`])
//! 2. `resolveIncludeDict` — dict-level `include:` keys
//! 3. `resolveSnippetDefinitions` — `snippets:` inheritance fixpoint
//! 4. `resolveSnippetRefs` — dict-level `snippet:` keys
//! 5. `resolveSnippetStrings` — `${snippet(name)}` expressions
//! 6. `resolveIncludeStrings` — `${include(path)}` expressions
//!
//! Template `${...}` expressions of any *other* name (`${state.*}`,
//! `${config.*}`, …) are not touched — they resolve at eval time in
//! the consumer.

mod expand;
mod extends;
mod parse;
mod tamias;

use std::collections::HashSet;
use std::path::Path;

use serde_json::{Map, Value};

pub use expand::deep_merge;
use expand::{
    resolve_include_dict, resolve_include_strings, resolve_snippet_definitions,
    resolve_snippet_refs, resolve_snippet_strings,
};
use parse::{parse_json, parse_toml, parse_yaml, read_text_file};
pub use tamias::TamiasRoots;

/// Composition failure — the analogue of peregrin's `LoadError`
/// (`reason` + optional `cause`, carried as `#[source]`). Error
/// messages match the TS/peregrin pipeline verbatim; tests assert on
/// [`reason`](Self::reason).
#[derive(Debug, thiserror::Error)]
#[error("{reason}")]
pub struct ComposeError {
    reason: String,
    #[source]
    cause: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl ComposeError {
    /// A reason with no underlying cause.
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            cause: None,
        }
    }

    /// A reason wrapping the I/O or parse error that caused it.
    pub fn caused(
        reason: impl Into<String>,
        cause: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            reason: reason.into(),
            cause: Some(Box::new(cause)),
        }
    }

    /// The message text — what the pipeline tests assert on.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// The composer — holds the `tamias:` resolution context and runs the
/// expansion pipeline over parsed documents.
///
/// [`Composer::new`] derives tamias roots from the environment
/// (`TAMIAS_HOME`/`XDG_DATA_HOME`/`HOME` for the store,
/// `TAMIAS_CACHE`/`XDG_CACHE_HOME`/`HOME` for the fetch cache — the
/// same chain `tamias-cli` uses); [`Composer::with_tamias_roots`]
/// pins both for embedded hosts and tests;
/// [`Composer::without_tamias`](Self::without_tamias) disables `tamias:`
/// refs entirely (they error loudly, never silently).
#[derive(Clone, Debug)]
pub struct Composer {
    tamias: Option<TamiasRoots>,
}

impl Composer {
    /// Env-derived tamias roots — the production shape.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tamias: Some(TamiasRoots::from_env()),
        }
    }

    /// Explicit store + fetch-cache roots for `tamias:` includes.
    #[must_use]
    pub fn with_tamias_roots(roots: TamiasRoots) -> Self {
        Self {
            tamias: Some(roots),
        }
    }

    /// Disable `tamias:` includes — any `tamias:` ref errors.
    #[must_use]
    pub fn without_tamias(&self) -> Self {
        Self { tamias: None }
    }

    /// Parse `text` as YAML and run the expansion pipeline, anchored
    /// at `base_dir` for relative includes.
    ///
    /// # Errors
    /// [`ComposeError`] on parse failure, unreadable/missing include
    /// targets, circular includes, or malformed `include:`/`snippet:`
    /// directives.
    pub fn resolve_yaml(&self, text: &str, base_dir: &Path) -> Result<Value, ComposeError> {
        self.expand(&parse_yaml(text)?, base_dir)
    }

    /// Parse `text` as TOML (converted to [`serde_json::Value`]) and
    /// run the expansion pipeline.
    ///
    /// # Errors
    /// Same as [`resolve_yaml`](Self::resolve_yaml).
    pub fn resolve_toml(&self, text: &str, base_dir: &Path) -> Result<Value, ComposeError> {
        self.expand(&parse_toml(text)?, base_dir)
    }

    /// Parse `text` as JSON and run the expansion pipeline.
    ///
    /// # Errors
    /// Same as [`resolve_yaml`](Self::resolve_yaml).
    pub fn resolve_json(&self, text: &str, base_dir: &Path) -> Result<Value, ComposeError> {
        self.expand(&parse_json(text)?, base_dir)
    }

    /// Read `path` and resolve it — dispatched on extension: `.json`
    /// parses as JSON, `.toml` as TOML, anything else as YAML.
    /// Includes inside the document anchor at the file's directory.
    ///
    /// # Errors
    /// Same as [`resolve_yaml`](Self::resolve_yaml), plus file-read
    /// failures.
    pub fn resolve_file(&self, path: &Path) -> Result<Value, ComposeError> {
        let text = read_text_file(path)?;
        let absolute = std::path::absolute(path).map_err(|e| {
            ComposeError::caused(format!("Failed to read file: {}", path.display()), e)
        })?;
        let base_dir = absolute.parent().unwrap_or_else(|| Path::new("."));
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("json") => self.resolve_json(&text, base_dir),
            Some("toml") => self.resolve_toml(&text, base_dir),
            _ => self.resolve_yaml(&text, base_dir),
        }
    }

    /// The expansion pipeline — `loadRaw` minus discovery and the
    /// typed-model pass, prefixed by document-level `extends:`
    /// resolution when the document declares parents.
    fn expand(&self, parsed: &Value, base_dir: &Path) -> Result<Value, ComposeError> {
        if let Some(merged) = extends::resolve(parsed, base_dir, self.tamias.as_ref())? {
            // Extends path: per-document include passes already ran
            // (each anchored at its own dir); the snippet passes run
            // once on the merged document.
            return Self::resolve_snippets(&merged);
        }
        let resolved =
            resolve_include_dict(parsed, base_dir, self.tamias.as_ref(), &mut HashSet::new())?;
        let resolved = Self::resolve_snippets(&resolved)?;
        resolve_include_strings(
            &resolved,
            base_dir,
            self.tamias.as_ref(),
            &mut HashSet::new(),
        )
    }

    /// The snippet passes — `snippets:` definitions fixpoint, then
    /// `snippet:` keys, then `${snippet(...)}` strings.
    fn resolve_snippets(resolved: &Value) -> Result<Value, ComposeError> {
        let snippets = match resolved.get("snippets") {
            None => Map::new(),
            Some(Value::Object(map)) => resolve_snippet_definitions(map)?,
            Some(_) => return Err(ComposeError::new("'snippets' must be a mapping")),
        };
        let resolved = resolve_snippet_refs(resolved, &snippets, &mut HashSet::new())?;
        resolve_snippet_strings(&resolved, &snippets, &mut HashSet::new())
    }
}

impl Default for Composer {
    fn default() -> Self {
        Self::new()
    }
}
