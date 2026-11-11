//! Recursive JSON merge — the pure half of layered config loading.
//! File sources parse each layer into a `serde_json::Value`, fold them
//! low→high precedence with [`merge_value`], then deserialize the
//! merged tree into the consumer's model. Merging happens on JSON,
//! never on the typed model, so it stays generic over `T`.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

/// Recursively merge `overlay` into `base`: JSON objects merge
/// key-wise (the overlay wins per key, recursing into shared object
/// values); scalars, arrays, and null **replace** — arrays never
/// concatenate.
///
/// ```
/// use scarab_config::merge_value;
/// use serde_json::json;
///
/// let mut base = json!({"a": 1, "b": {"x": 1, "y": 2}, "c": [1, 2]});
/// merge_value(&mut base, json!({"b": {"y": 3, "z": 4}, "c": [9]}));
/// assert_eq!(base, json!({"a": 1, "b": {"x": 1, "y": 3, "z": 4}, "c": [9]}));
/// ```
pub fn merge_value(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(b), Value::Object(o)) => {
            for (key, value) in o {
                match b.get_mut(&key) {
                    Some(slot) => merge_value(slot, value),
                    None => {
                        b.insert(key, value);
                    }
                }
            }
        }
        (slot, value) => *slot = value,
    }
}

/// Apply flat dotted-path overrides — `"server.port" → 8080` sets the
/// leaf at `server.port`, creating intermediate objects as needed.
/// `BTreeMap` order is deterministic (lexicographic), so `"a"` lands
/// before `"a.b"`; a scalar sitting where a path needs to descend is
/// replaced by an object. Empty path segments are literal keys.
///
/// This is the programmatic highest-precedence layer: call after the
/// file layers are merged.
pub fn apply_overrides(base: &mut Value, overrides: &BTreeMap<String, Value>) {
    if !overrides.is_empty() && !matches!(base, Value::Object(_)) {
        *base = Value::Object(Map::new());
    }
    for (path, value) in overrides {
        let keys: Vec<&str> = path.split('.').collect();
        set_dotted(base, &keys, value);
    }
}

/// Descend `keys` from `node`, materializing objects, and set the
/// leaf to `value` (replace semantics).
fn set_dotted(node: &mut Value, keys: &[&str], value: &Value) {
    let Some((&key, rest)) = keys.split_first() else {
        return;
    };
    // a scalar/array/null where the path needs an object is replaced —
    // the override wins
    if !matches!(node, Value::Object(_)) {
        *node = Value::Object(Map::new());
    }
    let Value::Object(map) = node else {
        return;
    };
    if rest.is_empty() {
        map.insert(key.to_string(), value.clone());
    } else {
        set_dotted(
            map.entry(key.to_string()).or_insert(Value::Null),
            rest,
            value,
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn objects_merge_key_wise_recursively() {
        let mut base = json!({
            "server": {"host": "a", "port": 1},
            "keep": true
        });
        merge_value(
            &mut base,
            json!({"server": {"port": 2, "tls": true}, "added": 1}),
        );
        assert_eq!(
            base,
            json!({
                "server": {"host": "a", "port": 2, "tls": true},
                "keep": true,
                "added": 1
            })
        );
    }

    #[test]
    fn scalars_and_arrays_replace() {
        let mut base = json!({"n": 1, "s": "x", "arr": [1, 2, 3], "nil": null});
        merge_value(&mut base, json!({"n": 2, "s": "y", "arr": [9], "nil": 5}));
        assert_eq!(base, json!({"n": 2, "s": "y", "arr": [9], "nil": 5}));
        // non-object base is wholesale replaced
        let mut base = json!("scalar");
        merge_value(&mut base, json!({"a": 1}));
        assert_eq!(base, json!({"a": 1}));
        let mut base = json!({"a": 1});
        merge_value(&mut base, json!([1, 2]));
        assert_eq!(base, json!([1, 2]));
    }

    #[test]
    fn overlay_null_replaces() {
        let mut base = json!({"a": {"b": 1}, "c": 2});
        merge_value(&mut base, json!({"a": null}));
        assert_eq!(base, json!({"a": null, "c": 2}));
    }

    #[test]
    fn dotted_overrides_set_leaves() {
        let mut base = json!({"server": {"host": "a"}, "top": 1});
        let overrides = BTreeMap::from([
            ("server.port".to_string(), json!(8080)),
            ("server.tls.enabled".to_string(), json!(true)),
            ("brand.new.deep".to_string(), json!("x")),
        ]);
        apply_overrides(&mut base, &overrides);
        assert_eq!(
            base,
            json!({
                "server": {"host": "a", "port": 8080, "tls": {"enabled": true}},
                "top": 1,
                "brand": {"new": {"deep": "x"}}
            })
        );
    }

    #[test]
    fn dotted_overrides_replace_scalars_in_the_way() {
        let mut base = json!({"a": 5});
        let overrides = BTreeMap::from([("a.b".to_string(), json!(1))]);
        apply_overrides(&mut base, &overrides);
        assert_eq!(base, json!({"a": {"b": 1}}));
    }

    #[test]
    fn apply_overrides_on_non_object_initializes() {
        let mut base = Value::Null;
        let overrides = BTreeMap::from([("k".to_string(), json!(1))]);
        apply_overrides(&mut base, &overrides);
        assert_eq!(base, json!({"k": 1}));
        // empty map leaves base untouched
        let mut base = json!("keep");
        apply_overrides(&mut base, &BTreeMap::new());
        assert_eq!(base, json!("keep"));
    }
}
