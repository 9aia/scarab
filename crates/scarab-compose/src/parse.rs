//! Parse helpers — `readTextFile`/`parseYaml`/`parseJson` ports, plus
//! TOML.
//!
//! YAML parses through `serde_yaml_ng::Value` (which allows non-string
//! keys) and converts to `serde_json::Value`, coercing map keys the way
//! the JS `yaml` package's plain-object output does — `emit: {0: …}`
//! lands as `"0"`. TOML parses through `toml::Value` and converts the
//! same way (datetimes stringify, like JS `String(v)`).

use std::path::Path;

use serde_json::{Map, Value};

use crate::ComposeError;

/// TS `readTextFile` — `Failed to read file: <path>` with the io error
/// as the cause.
pub(crate) fn read_text_file(path: &Path) -> Result<String, ComposeError> {
    std::fs::read_to_string(path)
        .map_err(|e| ComposeError::caused(format!("Failed to read file: {}", path.display()), e))
}

/// TS `parseYaml` — the error's `Display` carries line/column like the
/// `yaml` package's `YAMLError`.
pub(crate) fn parse_yaml(text: &str) -> Result<Value, ComposeError> {
    let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(text)
        .map_err(|e| ComposeError::caused("Failed to parse YAML", e))?;
    Ok(yaml_to_json(&yaml))
}

/// TS `parseJson` — used for `*.event.json` payloads.
pub(crate) fn parse_json(text: &str) -> Result<Value, ComposeError> {
    serde_json::from_str(text).map_err(|e| ComposeError::caused("Failed to parse JSON", e))
}

/// TOML → [`serde_json::Value`]. `toml::Value` doesn't map 1:1 (it has
/// a `Datetime` variant JSON lacks), so the conversion is explicit —
/// datetimes stringify like JS `String(v)`, non-finite floats become
/// `null` (JSON has no NaN/Infinity).
pub(crate) fn parse_toml(text: &str) -> Result<Value, ComposeError> {
    let doc: toml::Value =
        toml::from_str(text).map_err(|e| ComposeError::caused("Failed to parse TOML", e))?;
    Ok(toml_to_json(&doc))
}

fn toml_to_json(value: &toml::Value) -> Value {
    match value {
        toml::Value::String(s) => Value::String(s.clone()),
        toml::Value::Integer(i) => Value::Number((*i).into()),
        toml::Value::Float(f) => {
            serde_json::Number::from_f64(*f).map_or(Value::Null, Value::Number)
        }
        toml::Value::Boolean(b) => Value::Bool(*b),
        toml::Value::Datetime(dt) => Value::String(dt.to_string()),
        toml::Value::Array(items) => Value::Array(items.iter().map(toml_to_json).collect()),
        toml::Value::Table(table) => {
            let mut out = Map::with_capacity(table.len());
            for (key, val) in table {
                out.insert(key.clone(), toml_to_json(val));
            }
            Value::Object(out)
        }
    }
}

/// JSON-map key coercion matching JS object-key semantics (`String(k)`).
fn key_string(value: &serde_yaml_ng::Value) -> String {
    use serde_yaml_ng::Value as Y;
    match value {
        Y::Null => "null".to_string(),
        Y::Bool(b) => b.to_string(),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else {
                n.as_f64().map_or_else(
                    || n.to_string(),
                    |f| {
                        if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e21 {
                            format!("{}", f as i64)
                        } else {
                            n.to_string()
                        }
                    },
                )
            }
        }
        Y::String(s) => s.clone(),
        Y::Sequence(items) => items.iter().map(key_string).collect::<Vec<_>>().join(","),
        // Non-scalar keys coerce like JS `[object Object]`; tagged keys
        // unwrap to their inner value.
        Y::Mapping(_) => "[object Object]".to_string(),
        Y::Tagged(tagged) => key_string(&tagged.value),
    }
}

fn yaml_to_json(value: &serde_yaml_ng::Value) -> Value {
    use serde_yaml_ng::Value as Y;
    match value {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(*b),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(i.into())
            } else if let Some(u) = n.as_u64() {
                Value::Number(u.into())
            } else {
                n.as_f64()
                    .and_then(serde_json::Number::from_f64)
                    .map_or(Value::Null, Value::Number)
            }
        }
        Y::String(s) => Value::String(s.clone()),
        Y::Sequence(items) => Value::Array(items.iter().map(yaml_to_json).collect()),
        Y::Mapping(map) => {
            let mut out = Map::with_capacity(map.len());
            for (k, v) in map {
                out.insert(key_string(k), yaml_to_json(v));
            }
            Value::Object(out)
        }
        Y::Tagged(tagged) => yaml_to_json(&tagged.value),
    }
}
