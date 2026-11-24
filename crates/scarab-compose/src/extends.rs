//! Document-level `extends:` — the COMPOSITION.md chain semantics
//! (`scarab-template/COMPOSITION.md`) adapted from template packs to
//! whole documents.
//!
//! A document may carry a top-level `extends:` key:
//!
//! ```yaml
//! extends: base.yaml            # one parent
//! extends: [platform.yaml, org/overlay.yaml]  # several, lowest
//!                                             # precedence first
//! extends: {source: tamias:kit//base.yaml, via: tamias-ref}
//! ```
//!
//! Semantics (mirroring the pack contract):
//!
//! - `extends` absent or `null` → no parents. `extends: []` is the
//!   same — the key is still consumed.
//! - Array order is declaration order = **lowest precedence first**;
//!   later entries win merges. A structured `{source, via}` entry
//!   resolves `source` as the locator; `via` is validated against the
//!   known schemes (`tamias`/`tamias-ref` — the only resolution
//!   scheme this composer implements, covering both plain paths and
//!   `tamias:` refs) and unknown schemes error, per the contract's
//!   "never silently fall back" rule. `via` does not change how
//!   `source` is interpreted — the locator grammar stays with
//!   [`resolve_include_target`].
//! - Parents resolve through the same locator machinery as
//!   `include:`/`tamias:` — plain paths anchor at the extending
//!   document's `base_dir`, `tamias:<spec>//<path>` refs resolve
//!   through the tamias store (fetch-on-miss included).
//! - Parent documents parse by **extension dispatch**, like
//!   [`Composer::resolve_file`](crate::Composer::resolve_file):
//!   `.json` → JSON, `.toml` → TOML, anything else → YAML (a YAML
//!   superset covers `.yaml`/`.yml`/extensionless).
//! - Composition is **ancestor-first** (post-order): each parent's
//!   own ancestors compose before it, the document itself last. A
//!   shared ancestor reachable through several paths composes **once**,
//!   at its first occurrence — diamonds collapse to tree order
//!   (tracked by canonical resolved path in [`State::composed`]).
//! - A cycle (`a.yaml` ⇄ `b.yaml`, or self-extends) errors via
//!   [`State::on_stack`], the active resolution stack — distinct from
//!   the diamond-collapse set.
//! - Ancestor nesting caps at [`MAX_DEPTH`] (COMPOSITION.md's
//!   `merge.max-depth` default of 8); exceeding it errors.
//! - Merging folds the ordered ancestor-first chain with
//!   [`deep_merge`]: each entry merges *over* everything before it
//!   (later wins; dicts per-key, arrays concat later-first… see
//!   `deep_merge`), so the child document wins over all ancestors.
//!
//! Placement in the pipeline: extends resolution runs **first**,
//! before `include:` expansion — but each document body gets its
//! location-sensitive passes (`include:` dicts and `${include(...)}`
//! strings) applied while still anchored at *its own* directory, so a
//! parent's includes resolve relative to the parent's dir (mirroring
//! how includes inside included files anchor). The snippet passes
//! (`snippets:` definitions, `snippet:` keys, `${snippet(...)}`)
//! then run once on the merged document — snippet namespaces compose
//! across the chain like any other key.
//!
//! `extends:` is consumed — it never appears in resolved output.
//! Only the *document's* top-level key is honored; an `extends:` key
//! injected deeper (e.g. via an `include:` fragment merging into the
//! top level) remains inert data.

use std::collections::HashSet;
use std::path::Path;

use serde_json::Value;

use crate::ComposeError;
use crate::expand::{
    deep_merge, dirname, resolve_include_dict, resolve_include_strings, resolve_include_target,
};
use crate::parse::{parse_json, parse_toml, parse_yaml, read_text_file};
use crate::tamias::TamiasRoots;

/// COMPOSITION.md `merge.max-depth` default — caps ancestor nesting.
/// A document at depth 8 may not declare further parents.
pub(crate) const MAX_DEPTH: usize = 8;

/// Resolution bookkeeping shared across one document's whole extends
/// forest.
#[derive(Default)]
struct State {
    /// Canonical paths whose extends-subtree finished composing —
    /// diamonds collapse to their first occurrence.
    composed: HashSet<String>,
    /// Canonical paths on the active resolution stack — a repeat is
    /// a cycle, not a diamond.
    on_stack: HashSet<String>,
}

/// Resolve `doc`'s top-level `extends:` chain.
///
/// Returns `Ok(None)` when the document carries no `extends` key —
/// the zero-cost path, behavior identical to before. `Ok(Some(_))`
/// is the merged document (`extends` consumed, per-document include
/// passes applied) ready for the snippet passes.
pub(crate) fn resolve(
    doc: &Value,
    base_dir: &Path,
    tamias: Option<&TamiasRoots>,
) -> Result<Option<Value>, ComposeError> {
    let has_extends = matches!(doc, Value::Object(map) if map.contains_key("extends"));
    if !has_extends {
        return Ok(None);
    }
    let mut state = State::default();
    let mut chain: Vec<Value> = Vec::new();
    collect(doc, base_dir, tamias, &mut state, 0, &mut chain)?;
    // Fold ancestor-first: each entry is `local` over the accumulated
    // earlier (lower-precedence) entries, so later wins — the child
    // document, emitted last, wins over all ancestors.
    let mut merged = Value::Null;
    for body in chain {
        merged = deep_merge(&body, &merged);
    }
    Ok(Some(merged))
}

/// Post-order walk: resolve `doc`'s parents (each recursively), then
/// emit `doc`'s own extends-stripped, include-expanded body. Bodies
/// land ancestor-first in `chain`.
fn collect(
    doc: &Value,
    base_dir: &Path,
    tamias: Option<&TamiasRoots>,
    state: &mut State,
    depth: usize,
    chain: &mut Vec<Value>,
) -> Result<(), ComposeError> {
    let (body, locators) = take_extends(doc)?;
    if locators.is_empty() {
        chain.push(preprocess(&body, base_dir, tamias)?);
        return Ok(());
    }
    if depth >= MAX_DEPTH {
        return Err(ComposeError::new(format!(
            "extends chain exceeds maximum depth of {MAX_DEPTH}"
        )));
    }
    for locator in &locators {
        let full = resolve_include_target(locator, base_dir, tamias)?;
        let key = full.to_string_lossy().into_owned();
        if state.on_stack.contains(&key) {
            return Err(ComposeError::new(format!(
                "circular extends detected: {key}"
            )));
        }
        if state.composed.contains(&key) {
            continue;
        }
        let text = read_text_file(&full)?;
        let parsed = parse_parent(&text, &full)?;
        let parent_dir = dirname(&full).to_path_buf();
        state.on_stack.insert(key.clone());
        let result = collect(&parsed, &parent_dir, tamias, state, depth + 1, chain);
        state.on_stack.remove(&key);
        result?;
        state.composed.insert(key);
    }
    chain.push(preprocess(&body, base_dir, tamias)?);
    Ok(())
}

/// Split `doc` into (body without `extends`, parent locators).
/// Non-object docs and objects without the key pass through with an
/// empty locator list; `extends: null`/`[]` consume the key with no
/// parents.
fn take_extends(doc: &Value) -> Result<(Value, Vec<String>), ComposeError> {
    let Value::Object(map) = doc else {
        return Ok((doc.clone(), Vec::new()));
    };
    let Some(extends) = map.get("extends") else {
        return Ok((doc.clone(), Vec::new()));
    };
    let mut body = map.clone();
    body.remove("extends");
    let locators = match extends {
        Value::Null => Vec::new(),
        Value::String(_) | Value::Object(_) => vec![locator(extends)?],
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(locator(item)?);
            }
            out
        }
        other => {
            return Err(ComposeError::new(format!(
                "extends must be a source string, a list of sources, or a {{source, via}} object; \
                 got {}",
                json_kind(other)
            )));
        }
    };
    Ok((Value::Object(body), locators))
}

/// One `extends` entry → its locator string. Strings are locators
/// verbatim; `{source, via}` objects take `source` (a non-empty
/// string) and validate `via` against the known schemes.
fn locator(entry: &Value) -> Result<String, ComposeError> {
    match entry {
        Value::String(source) => {
            if source.trim().is_empty() {
                return Err(ComposeError::new("extends source must not be empty"));
            }
            Ok(source.clone())
        }
        Value::Object(map) => {
            let source = map.get("source").ok_or_else(|| {
                ComposeError::new("extends entry object needs a 'source' locator")
            })?;
            let source = source
                .as_str()
                .ok_or_else(|| ComposeError::new("extends 'source' must be a string"))?;
            if source.trim().is_empty() {
                return Err(ComposeError::new("extends source must not be empty"));
            }
            if let Some(via) = map.get("via") {
                match via.as_str() {
                    // The ambient resolver's schemes — plain paths and
                    // `tamias:` refs are both handled by
                    // `resolve_include_target`; `via` selects nothing
                    // new, it only names what the source already is.
                    Some("tamias" | "tamias-ref") => {}
                    Some(other) => {
                        return Err(ComposeError::new(format!(
                            "unsupported extends 'via' scheme '{other}'"
                        )));
                    }
                    None => {
                        return Err(ComposeError::new("extends 'via' must be a string"));
                    }
                }
            }
            Ok(source.to_string())
        }
        other => Err(ComposeError::new(format!(
            "extends entries must be strings or {{source, via}} objects; got {}",
            json_kind(other)
        ))),
    }
}

/// Parse a parent document — extension dispatch like
/// [`Composer::resolve_file`](crate::Composer::resolve_file).
fn parse_parent(text: &str, path: &Path) -> Result<Value, ComposeError> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("json") => parse_json(text),
        Some("toml") => parse_toml(text),
        _ => parse_yaml(text),
    }
}

/// The location-sensitive passes for one document body, anchored at
/// that document's own directory: `include:` dicts then
/// `${include(...)}` strings. Snippet passes stay global — they run
/// on the merged document in `Composer::expand`.
fn preprocess(
    body: &Value,
    base_dir: &Path,
    tamias: Option<&TamiasRoots>,
) -> Result<Value, ComposeError> {
    let resolved = resolve_include_dict(body, base_dir, tamias, &mut HashSet::new())?;
    resolve_include_strings(&resolved, base_dir, tamias, &mut HashSet::new())
}

/// Short type name for error messages (`"a number"`, `"an array"`…).
fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}
