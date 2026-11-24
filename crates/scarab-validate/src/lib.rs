//! scarab-validate — the schema-validation leg: a compiled JSON
//! Schema (draft 2020-12) checked against [`serde_json::Value`]
//! instances.
//!
//! Consumers bring `Value` documents — output from
//! [`scarab-compose`](../scarab_compose/index.html) pipelines, YAML/TOML
//! conversions, hand-built maps — and their own schema documents; scarab
//! never names a consumer's type. Compile once with [`Schema::compile`],
//! validate many with [`Schema::validate`].
//!
//! Ported from peregrin's `peregrin-loader` `params_violations`
//! (bundle `config:` validation), generalized: the schema is any JSON
//! Schema document the caller supplies, the instance is any `Value`,
//! and violations come back structured (`path` + `message`) rather than
//! pre-formatted — callers compose them however their surface needs.
//!
//! Convention note: peregrin names its schema file
//! `params.schema.json`; [`PARAMS_SCHEMA_FILE`] re-exports that name
//! for drop-in compatibility, but scarab itself is convention-free —
//! consumers choose where their schema document lives.

use std::fmt;

/// The conventional schema file name peregrin bundles drop beside
/// `main.yaml` — re-exported here for drop-in compatibility. Scarab
/// itself is convention-free; consumers choose their own file layout.
pub const PARAMS_SCHEMA_FILE: &str = "params.schema.json";

/// A compiled JSON Schema (draft 2020-12) — compile once, validate many.
pub struct Schema {
    validator: jsonschema::Validator,
}

impl Schema {
    /// Compile `schema` (a [`serde_json::Value`]).
    ///
    /// # Errors
    /// Returns `Err` when `schema` is not a usable JSON Schema — a
    /// broken schema is a caller bug and must surface, never pass.
    pub fn compile(schema: &serde_json::Value) -> Result<Self, String> {
        let validator = jsonschema::validator_for(schema)
            .map_err(|e| format!("schema is not a usable JSON Schema: {e}"))?;
        Ok(Self { validator })
    }

    /// Validate `instance` (any [`serde_json::Value`]). Empty vec = valid.
    #[must_use]
    pub fn validate(&self, instance: &serde_json::Value) -> Vec<Violation> {
        self.validator
            .iter_errors(instance)
            .map(|e| {
                let path = e.instance_path().to_string();
                Violation {
                    path,
                    message: e.to_string(),
                }
            })
            .collect()
    }
}

/// One schema violation — `path` is the instance_path (`""` for root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// The JSON Pointer-ish instance path where the violation occurred.
    /// Empty string for the document root.
    pub path: String,
    /// The validator's error message.
    pub message: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use serde_json::{Map, Value, json};

    use super::*;

    fn schema() -> Value {
        json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "additionalProperties": false,
            "required": ["token_env"],
            "properties": {
                "token_env": {"type": "string", "pattern": "^[A-Z_]+$"},
                "timeout": {"type": "integer", "minimum": 1, "maximum": 600},
                "repos": {"type": "array", "items": {"type": "string"}}
            }
        })
    }

    fn cfg_map(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn valid_config_has_no_violations() {
        let config = cfg_map(&[
            ("token_env", json!("BUPHA_TOKEN")),
            ("timeout", json!(600)),
            ("repos", json!(["9aia/peregrin"])),
        ]);
        let schema = Schema::compile(&schema()).unwrap();
        assert_eq!(schema.validate(&Value::Object(config)), Vec::new());
    }

    #[test]
    fn typoed_key_rejects_under_additional_properties_false() {
        let config = cfg_map(&[("token_env", json!("BUPHA_TOKEN")), ("timout", json!(600))]);
        let schema = Schema::compile(&schema()).unwrap();
        let errors = schema.validate(&Value::Object(config));
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].to_string().contains("timout"), "{errors:?}");
    }

    #[test]
    fn missing_required_rejects() {
        let config = cfg_map(&[("timeout", json!(60))]);
        let schema = Schema::compile(&schema()).unwrap();
        let errors = schema.validate(&Value::Object(config));
        assert!(
            errors.iter().any(|e| e.to_string().contains("token_env")),
            "{errors:?}"
        );
    }

    #[test]
    fn wrong_type_and_bounds_reject() {
        let schema = Schema::compile(&schema()).unwrap();

        let config = cfg_map(&[
            ("token_env", json!("BUPHA_TOKEN")),
            ("timeout", json!("600")),
        ]);
        let errors = schema.validate(&Value::Object(config));
        assert!(
            errors.iter().any(|e| e.to_string().contains("timeout")),
            "{errors:?}"
        );

        let config = cfg_map(&[("token_env", json!("BUPHA_TOKEN")), ("timeout", json!(0))]);
        let errors = schema.validate(&Value::Object(config));
        assert!(
            errors.iter().any(|e| e.to_string().contains("minimum")),
            "{errors:?}"
        );

        let config = cfg_map(&[("token_env", json!("lowercase"))]);
        let errors = schema.validate(&Value::Object(config));
        assert!(
            errors
                .iter()
                .any(|e| e.to_string().contains("does not match")),
            "{errors:?}"
        );
    }

    #[test]
    fn unusable_schema_is_an_error_not_a_pass() {
        let bad = json!({"type": "not-a-type"});
        assert!(Schema::compile(&bad).is_err());
    }

    #[test]
    fn display_prefixes_path_and_leaves_root_bare() {
        // A nested violation carries its path: "path: message".
        let nested = Violation {
            path: "/repos/0".to_string(),
            message: "expected a string".to_string(),
        };
        assert_eq!(nested.to_string(), "/repos/0: expected a string");

        // A root violation has no prefix.
        let root = Violation {
            path: String::new(),
            message: "required property missing".to_string(),
        };
        assert_eq!(root.to_string(), "required property missing");
    }
}
