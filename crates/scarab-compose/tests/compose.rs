//! Composition tests — the generic cases ported from
//! `peregrin-loader-yaml/tests/loader.rs`, asserting on the resolved
//! `serde_json::Value` rather than a `Campaign`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

#[cfg(feature = "tamias")]
use scarab_compose::TamiasRoots;
use scarab_compose::{ComposeError, Composer, deep_merge};
use serde_json::{Value, json};

fn composer() -> Composer {
    Composer::new()
}

fn load(text: &str, dir: &Path) -> Result<Value, ComposeError> {
    composer().resolve_yaml(text, dir)
}

#[test]
fn resolves_a_minimal_document() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = load("name: test\nnodes: []", tmp.path()).unwrap();
    assert_eq!(doc["name"], json!("test"));
    assert_eq!(doc["nodes"], json!([]));
}

#[test]
fn passes_non_mapping_documents_through() {
    let tmp = tempfile::tempdir().unwrap();
    // Unlike the campaign loader there is no "must be a mapping" gate —
    // the composer is format-agnostic.
    let doc = load("- a\n- b", tmp.path()).unwrap();
    assert_eq!(doc, json!(["a", "b"]));
    let doc = load("just a scalar", tmp.path()).unwrap();
    assert_eq!(doc, json!("just a scalar"));
}

#[test]
fn merges_include_into_node_and_local_wins() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("env.yaml"),
        "env:\n  REGION: us-east-1\ntimeout: 300\n",
    )
    .unwrap();
    let doc = load(
        "nodes:\n  - id: deploy\n    params:\n      include: env.yaml\n      cmd: deploy.sh",
        tmp.path(),
    )
    .unwrap();
    let params = &doc["nodes"][0]["params"];
    assert_eq!(params["env"], json!({"REGION": "us-east-1"}));
    assert_eq!(params["timeout"], 300);
    assert_eq!(params["cmd"], "deploy.sh");
    assert!(params.get("include").is_none());
}

#[test]
fn injects_a_whole_dict_via_include_expression() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("env.yaml"),
        "REGION: us-east-1\nCLUSTER: prod\n",
    )
    .unwrap();
    let doc = load(
        "nodes:\n  - id: deploy\n    params:\n      env: ${include(env.yaml)}\n      cmd: deploy.sh",
        tmp.path(),
    )
    .unwrap();
    assert_eq!(
        doc["nodes"][0]["params"]["env"],
        json!({"REGION": "us-east-1", "CLUSTER": "prod"})
    );
}

#[test]
fn injects_a_whole_list_via_include_expression() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("items.yaml"),
        "- id: us\n  driver: shell\n- id: eu\n  driver: shell\n",
    )
    .unwrap();
    let doc = load("nodes: ${include(items.yaml)}", tmp.path()).unwrap();
    assert_eq!(doc["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(doc["nodes"][0]["id"], "us");
    assert_eq!(doc["nodes"][1]["id"], "eu");
}

#[test]
fn injects_scalar_and_partial_include_expressions() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("version.yaml"), "1.2.3").unwrap();
    let doc = load(
        "name: deploy-${include(version.yaml)}\ncmd: echo version ${include(version.yaml)}",
        tmp.path(),
    )
    .unwrap();
    assert_eq!(doc["name"], "deploy-1.2.3");
    assert_eq!(doc["cmd"], "echo version 1.2.3");
}

#[test]
fn uses_first_include_when_include_is_a_list() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.yaml"), "id: node-a\nx: 1\n").unwrap();
    fs::write(tmp.path().join("b.yaml"), "id: node-b\nx: 2\n").unwrap();
    let doc = load("nodes:\n  - include: [a.yaml, b.yaml]", tmp.path()).unwrap();
    assert_eq!(doc["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(doc["nodes"][0]["id"], "node-a");
    assert_eq!(doc["nodes"][0]["x"], 1);
}

#[test]
fn recursively_merges_include_files() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("base.yaml"), "env:\n  PYTHONPATH: /app\n").unwrap();
    fs::write(
        tmp.path().join("prod.yaml"),
        "include: base.yaml\nenv:\n  REGION: us-east-1\n",
    )
    .unwrap();
    let doc = load("params:\n  include: prod.yaml", tmp.path()).unwrap();
    assert_eq!(
        doc["params"]["env"],
        json!({"PYTHONPATH": "/app", "REGION": "us-east-1"})
    );
}

#[test]
fn resolves_recursive_include_expressions() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("base.yaml"), "PYTHONPATH: /app\n").unwrap();
    fs::write(
        tmp.path().join("prod.yaml"),
        "include: base.yaml\nREGION: us-east-1\n",
    )
    .unwrap();
    let doc = load("env: ${include(prod.yaml)}", tmp.path()).unwrap();
    assert_eq!(doc["env"]["PYTHONPATH"], "/app");
    assert_eq!(doc["env"]["REGION"], "us-east-1");
}

#[test]
fn resolves_include_expressions_inside_included_files() {
    // Includes anchor at the included file's directory — a nested
    // relative path resolves against *its* dir, not the root's.
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("sub")).unwrap();
    fs::write(tmp.path().join("sub/inner.yaml"), "deep: true\n").unwrap();
    fs::write(
        tmp.path().join("sub/mid.yaml"),
        "inner: ${include(inner.yaml)}\n",
    )
    .unwrap();
    let doc = load("mid: ${include(sub/mid.yaml)}", tmp.path()).unwrap();
    assert_eq!(doc["mid"]["inner"]["deep"], true);
}

#[test]
fn detects_circular_includes_by_key() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.yaml"), "include: b.yaml\nvalue: a\n").unwrap();
    fs::write(tmp.path().join("b.yaml"), "include: a.yaml\nvalue: b\n").unwrap();
    let err = composer()
        .resolve_yaml("include: a.yaml", tmp.path())
        .unwrap_err();
    assert!(err.reason().starts_with("circular include detected:"));
}

#[test]
fn detects_circular_includes_by_expression() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.yaml"), "value: ${include(b.yaml)}\n").unwrap();
    fs::write(tmp.path().join("b.yaml"), "value: ${include(a.yaml)}\n").unwrap();
    let err = composer()
        .resolve_yaml("val: ${include(a.yaml)}", tmp.path())
        .unwrap_err();
    assert!(err.reason().starts_with("circular include detected:"));
}

#[test]
fn detects_self_includes() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("main.yaml"), "include: main.yaml\n").unwrap();
    let err = composer()
        .resolve_yaml("include: main.yaml", tmp.path())
        .unwrap_err();
    assert!(err.reason().starts_with("circular include detected:"));
}

#[test]
fn raises_for_missing_include_files() {
    let tmp = tempfile::tempdir().unwrap();
    let err = load("params:\n  include: missing.yaml", tmp.path()).unwrap_err();
    assert!(err.reason().starts_with("Failed to read file:"));
}

#[test]
fn rejects_non_string_include_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let err = composer()
        .resolve_yaml("include: [a.yaml, 5]", tmp.path())
        .unwrap_err();
    // a.yaml is missing → read error fires first, matching TS order.
    assert!(err.reason().starts_with("Failed to read file:"));
    fs::write(tmp.path().join("a.yaml"), "x: 1\n").unwrap();
    let err = composer()
        .resolve_yaml("include: [a.yaml, 5]", tmp.path())
        .unwrap_err();
    assert_eq!(err.reason(), "include path must be a string");
}

#[test]
fn merges_a_snippet_into_a_node() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = load(
        "snippets:\n  base:\n    driver: shell\n    emits: [base.done]\n    params:\n      timeout: 600\n      env:\n        A: 1\nnodes:\n  - id: n\n    snippet: base\n    on: [start]\n    params:\n      env:\n        B: 2",
        tmp.path(),
    )
    .unwrap();
    let node = &doc["nodes"][0];
    assert_eq!(node["driver"], "shell");
    assert_eq!(node["emits"], json!(["base.done"]));
    assert_eq!(node["params"]["timeout"], 600);
    assert_eq!(node["params"]["env"], json!({"A": 1, "B": 2}));
}

#[test]
fn injects_a_scalar_snippet_via_expression() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = load(
        "snippets:\n  cmd: echo hello\nnodes:\n  - id: n\n    params:\n      cmd: ${snippet(cmd)}",
        tmp.path(),
    )
    .unwrap();
    assert_eq!(doc["nodes"][0]["params"]["cmd"], "echo hello");
}

#[test]
fn injects_a_list_snippet_via_expression() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = load(
        "snippets:\n  triggers: [pr.opened, pr.updated]\nnodes:\n  - id: n\n    on: ${snippet(triggers)}",
        tmp.path(),
    )
    .unwrap();
    assert_eq!(doc["nodes"][0]["on"], json!(["pr.opened", "pr.updated"]));
}

#[test]
fn injects_a_dict_snippet_via_expression() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = load(
        "snippets:\n  env:\n    REGION: us-east-1\n    CLUSTER: prod\nnodes:\n  - id: n\n    params:\n      env: ${snippet(env)}",
        tmp.path(),
    )
    .unwrap();
    assert_eq!(
        doc["nodes"][0]["params"]["env"],
        json!({"REGION": "us-east-1", "CLUSTER": "prod"})
    );
}

#[test]
fn interpolates_a_snippet_into_a_partial_string() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = load(
        "snippets:\n  model: sonnet\nnodes:\n  - id: n\n    params:\n      cmd: run --model ${snippet(model)}",
        tmp.path(),
    )
    .unwrap();
    assert_eq!(doc["nodes"][0]["params"]["cmd"], "run --model sonnet");
}

#[test]
fn raises_for_unknown_snippet_references() {
    let tmp = tempfile::tempdir().unwrap();
    let err = load(
        "nodes:\n  - id: n\n    snippet: missing\n    on: [start]",
        tmp.path(),
    )
    .unwrap_err();
    assert_eq!(err.reason(), "unknown snippet 'missing'");
}

#[test]
fn resolves_three_levels_of_snippet_inheritance() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = load(
        "snippets:\n  a:\n    params:\n      x: 1\n  b:\n    snippet: a\n    params:\n      y: 2\n  c:\n    snippet: b\n    params:\n      z: 3\nnodes:\n  - id: n\n    snippet: c\n    on: [start]",
        tmp.path(),
    )
    .unwrap();
    let params = &doc["nodes"][0]["params"];
    assert_eq!(params["x"], 1);
    assert_eq!(params["y"], 2);
    assert_eq!(params["z"], 3);
}

#[test]
fn detects_self_referencing_snippet_inheritance() {
    let tmp = tempfile::tempdir().unwrap();
    let err = load(
        "snippets:\n  loop:\n    snippet: loop\nnodes:\n  - id: n\n    snippet: loop\n    on: [start]",
        tmp.path(),
    )
    .unwrap_err();
    assert_eq!(
        err.reason(),
        "circular snippet: 'loop' inherits from itself"
    );
}

#[test]
fn detects_circular_snippet_references() {
    let tmp = tempfile::tempdir().unwrap();
    // a ⇄ b through `${snippet()}` strings inside snippet bodies.
    let err = load(
        "snippets:\n  a:\n    x: ${snippet(b)}\n  b:\n    x: ${snippet(a)}\nnodes:\n  - id: n\n    snippet: a\n    on: [start]",
        tmp.path(),
    )
    .unwrap_err();
    assert!(
        err.reason().starts_with("circular snippet reference:"),
        "{err}"
    );
}

#[test]
fn rejects_non_dict_snippets_table() {
    let tmp = tempfile::tempdir().unwrap();
    let err = load("snippets: [a, b]\nnodes: []", tmp.path()).unwrap_err();
    assert_eq!(err.reason(), "'snippets' must be a mapping");
}

#[test]
fn snippet_inheritance_local_wins() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = load(
        "snippets:\n  base:\n    a: 1\n    b: 1\n  child:\n    snippet: base\n    b: 2\nx: ${snippet(child)}",
        tmp.path(),
    )
    .unwrap();
    assert_eq!(doc["x"], json!({"a": 1, "b": 2}));
}

#[test]
fn yaml_parse_errors_carry_position() {
    let tmp = tempfile::tempdir().unwrap();
    let err = load("name: ok\nnodes:\n  - id: [", tmp.path()).unwrap_err();
    assert_eq!(err.reason(), "Failed to parse YAML");
    let source = std::error::Error::source(&err).unwrap().to_string();
    assert!(source.contains("line"), "expected position in: {source}");
}

// --- TOML / JSON / file entry points ---------------------------------------

#[test]
fn resolves_toml_documents() {
    let tmp = tempfile::tempdir().unwrap();
    // Include fragments always parse as YAML — even from a TOML doc.
    fs::write(tmp.path().join("frag.yaml"), "timeout: 42\n").unwrap();
    let doc = composer()
        .resolve_toml(
            "[params]\ninclude = \"frag.yaml\"\ncmd = \"run\"\ncount = 3\n",
            tmp.path(),
        )
        .unwrap();
    assert_eq!(doc["params"]["timeout"], 42);
    assert_eq!(doc["params"]["cmd"], "run");
    assert_eq!(doc["params"]["count"], 3);
    assert!(doc["params"].get("include").is_none());
}

#[test]
fn resolves_json_documents() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("frag.json"), "{\"timeout\": 7}").unwrap();
    let doc = composer()
        .resolve_json(
            r#"{"params": {"include": "frag.json", "cmd": "run"}}"#,
            tmp.path(),
        )
        .unwrap();
    // Included files still parse as YAML (a YAML superset of JSON).
    assert_eq!(doc["params"]["timeout"], 7);
    assert_eq!(doc["params"]["cmd"], "run");
}

#[test]
fn toml_datetimes_and_types_convert() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = composer()
        .resolve_toml(
            "when = 1979-05-27T07:32:00Z\nname = \"x\"\nok = true\n",
            tmp.path(),
        )
        .unwrap();
    assert_eq!(doc["when"], json!("1979-05-27T07:32:00Z"));
    assert_eq!(doc["name"], json!("x"));
    assert_eq!(doc["ok"], json!(true));
}

#[test]
fn resolve_file_dispatches_on_extension() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("doc.yaml"), "a: 1\n").unwrap();
    fs::write(tmp.path().join("doc.toml"), "a = 2\n").unwrap();
    fs::write(tmp.path().join("doc.json"), "{\"a\": 3}").unwrap();
    let c = composer();
    assert_eq!(
        c.resolve_file(&tmp.path().join("doc.yaml")).unwrap()["a"],
        1
    );
    assert_eq!(
        c.resolve_file(&tmp.path().join("doc.toml")).unwrap()["a"],
        2
    );
    assert_eq!(
        c.resolve_file(&tmp.path().join("doc.json")).unwrap()["a"],
        3
    );
    // Includes inside resolve_file anchor at the file's directory.
    fs::write(tmp.path().join("frag.yaml"), "b: 9\n").unwrap();
    fs::write(tmp.path().join("main.yaml"), "include: frag.yaml\n").unwrap();
    assert_eq!(
        c.resolve_file(&tmp.path().join("main.yaml")).unwrap()["b"],
        9
    );
    // Missing file → read error.
    let err = c.resolve_file(&tmp.path().join("nope.yaml")).unwrap_err();
    assert!(err.reason().starts_with("Failed to read file:"));
}

// --- deep_merge -------------------------------------------------------------

#[test]
fn deep_merge_semantics() {
    // Local wins; dicts merge per-key; arrays concat local-first.
    let local = json!({"a": {"x": 1, "y": 2}, "list": [1], "s": "local"});
    let imported = json!({"a": {"y": 9, "z": 3}, "list": [2, 3], "s": "imported", "extra": true});
    assert_eq!(
        deep_merge(&local, &imported),
        json!({"a": {"x": 1, "y": 2, "z": 3}, "list": [1, 2, 3], "s": "local", "extra": true})
    );
    // Scalars take local; non-dict/non-array pairs take local.
    assert_eq!(deep_merge(&json!(1), &json!({"a": 2})), json!(1));
    assert_eq!(
        deep_merge(&json!({"k": "v"}), &Value::Null),
        json!({"k": "v"})
    );
}

// --- tamias: include refs ----------------------------------------------------
//
// `tamias:<name>[@<label>]//<path>` resolves against the tamias install
// store. Tests hand-write store entries (`.tamias-store.json` records —
// the same on-disk shape `tamias install` commits) into a tempdir and
// compose through `Composer::with_tamias_roots`.

/// Write a fake store entry — dir `<store>/<dir_name>/` carrying
/// `files` plus a minimal `.tamias-store.json` record.
#[cfg(feature = "tamias")]
fn fake_store_entry(
    store: &Path,
    dir_name: &str,
    name: &str,
    version: Option<&str>,
    installed_at: u64,
    files: &[(&str, &str)],
) {
    let entry = store.join(dir_name);
    fs::create_dir_all(&entry).unwrap();
    for (rel, text) in files {
        let path = entry.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let version_json = version.map_or_else(|| "null".to_string(), |v| format!("\"{v}\""));
    fs::write(
        entry.join(".tamias-store.json"),
        format!(
            "{{\"name\":\"{name}\",\"version\":{version_json},\"source_ref\":\"./x\",\
             \"resolved\":\"sha256:abc\",\"digest\":\"sha256:def\",\"installed_at\":{installed_at}}}"
        ),
    )
    .unwrap();
}

#[cfg(feature = "tamias")]
fn load_with_store(
    text: &str,
    dir: &Path,
    store: &Path,
    cache: &Path,
) -> Result<Value, ComposeError> {
    Composer::with_tamias_roots(TamiasRoots::new(store, cache)).resolve_yaml(text, dir)
}

#[cfg(feature = "tamias")]
#[test]
fn tamias_include_merges_a_bundle_fragment() {
    let tmp = tempfile::tempdir().unwrap();
    let store_tmp = tempfile::tempdir().unwrap();
    let store = store_tmp.path().join("store");
    let cache = store_tmp.path().join("cache");
    fake_store_entry(
        &store,
        "handl-gates@0.2.0",
        "handl-gates",
        Some("0.2.0"),
        1_700_000_000,
        &[("lib/frag.yaml", "timeout: 42\n")],
    );
    let doc = load_with_store(
        "params:\n  include: tamias:handl-gates//lib/frag.yaml\n  cmd: gate.sh",
        tmp.path(),
        &store,
        &cache,
    )
    .unwrap();
    assert_eq!(doc["params"]["timeout"], 42);
    assert_eq!(doc["params"]["cmd"], "gate.sh");
    assert!(doc["params"].get("include").is_none());
}

#[cfg(feature = "tamias")]
#[test]
fn tamias_include_version_pin_and_latest_installed() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let cache = tmp.path().join("cache");
    // 0.1.0 installed *after* 0.2.0 — an unpinned ref takes the most
    // recently installed entry, a pin takes the named label.
    fake_store_entry(
        &store,
        "handl-gates@0.2.0",
        "handl-gates",
        Some("0.2.0"),
        1_700_000_000,
        &[("frag.yaml", "timeout: 200\n")],
    );
    fake_store_entry(
        &store,
        "handl-gates@0.1.0",
        "handl-gates",
        Some("0.1.0"),
        1_700_000_100,
        &[("frag.yaml", "timeout: 100\n")],
    );
    let pinned = load_with_store(
        "params:\n  include: tamias:handl-gates@0.2.0//frag.yaml",
        tmp.path(),
        &store,
        &cache,
    )
    .unwrap();
    assert_eq!(pinned["params"]["timeout"], 200);
    let latest = load_with_store(
        "params:\n  include: tamias:handl-gates//frag.yaml",
        tmp.path(),
        &store,
        &cache,
    )
    .unwrap();
    assert_eq!(latest["params"]["timeout"], 100);
}

#[cfg(feature = "tamias")]
#[test]
fn tamias_include_expression_and_escape_guard() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let cache = tmp.path().join("cache");
    fake_store_entry(
        &store,
        "handl-gates@0.2.0",
        "handl-gates",
        Some("0.2.0"),
        1_700_000_000,
        &[("env.yaml", "REGION: us-east-1\n")],
    );
    // ${include(tamias:…)} whole-string form injects the parsed doc.
    let doc = load_with_store(
        "params:\n  env: ${include(tamias:handl-gates//env.yaml)}",
        tmp.path(),
        &store,
        &cache,
    )
    .unwrap();
    assert_eq!(doc["params"]["env"], json!({"REGION": "us-east-1"}));
    // `..` may not escape the bundle root.
    let err = load_with_store(
        "params:\n  include: tamias:handl-gates//../ghost@9.9/env.yaml",
        tmp.path(),
        &store,
        &cache,
    )
    .unwrap_err();
    assert!(err.reason().contains("escapes the bundle root"), "{err}");
}

#[cfg(feature = "tamias")]
#[test]
fn tamias_include_missing_bundle_names_tamias_install() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let cache = tmp.path().join("cache");
    fake_store_entry(
        &store,
        "handl-gates@0.2.0",
        "handl-gates",
        Some("0.2.0"),
        1_700_000_000,
        &[("frag.yaml", "timeout: 1\n")],
    );
    let err = load_with_store(
        "params:\n  include: tamias:ghost//frag.yaml",
        tmp.path(),
        &store,
        &cache,
    )
    .unwrap_err();
    assert!(err.reason().contains("'ghost' is not installed"), "{err}");
    assert!(err.reason().contains("tamias install"), "{err}");
    // A missing *file* inside an installed bundle is a plain read error.
    let err = load_with_store(
        "params:\n  include: tamias:handl-gates//missing.yaml",
        tmp.path(),
        &store,
        &cache,
    )
    .unwrap_err();
    assert!(err.reason().starts_with("Failed to read file:"), "{err}");
}

#[cfg(feature = "tamias")]
#[test]
fn tamias_include_fetches_an_fs_ref_on_store_miss() {
    let tmp = tempfile::tempdir().unwrap();
    // An installable fs ref — a package dir with a `tamias.yaml`
    // manifest and the fragment the include pulls.
    let pkg = tmp.path().join("pkg");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(
        pkg.join("tamias.yaml"),
        "name: fetched-bundle\nversion: 1.0.0\n",
    )
    .unwrap();
    fs::write(pkg.join("frag.yaml"), "timeout: 77\n").unwrap();
    let roots = TamiasRoots::new(tmp.path().join("store"), tmp.path().join("cache"));
    let spec = format!("{}", pkg.display());
    let doc = Composer::with_tamias_roots(roots.clone())
        .resolve_yaml(
            &format!("params:\n  include: tamias:{spec}//frag.yaml"),
            tmp.path(),
        )
        .unwrap();
    assert_eq!(doc["params"]["timeout"], 77);
    // The fetch committed a provenance-carrying store entry.
    let entries = tamias_store_local::Store::new(tmp.path().join("store"))
        .entries()
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].record.name, "fetched-bundle");
    assert_eq!(entries[0].record.source_ref, spec);
    // The source dir is gone — the *name* spec now resolves store-side.
    fs::remove_dir_all(&pkg).unwrap();
    let doc = Composer::with_tamias_roots(roots)
        .resolve_yaml(
            "params:\n  include: tamias:fetched-bundle//frag.yaml",
            tmp.path(),
        )
        .unwrap();
    assert_eq!(doc["params"]["timeout"], 77);
}

#[cfg(feature = "tamias")]
#[test]
fn tamias_include_fetch_failure_is_loud() {
    let tmp = tempfile::tempdir().unwrap();
    let roots = TamiasRoots::new(tmp.path().join("store"), tmp.path().join("cache"));
    let c = Composer::with_tamias_roots(roots);
    // Ref-shaped but unfetchable — a missing fs path fails the install.
    let err = c
        .resolve_yaml(
            "params:\n  include: tamias:/nonexistent-pkg-scarab-test//frag.yaml",
            tmp.path(),
        )
        .unwrap_err();
    assert!(err.reason().contains("fetch failed"), "{err}");
    assert!(
        err.reason().contains("/nonexistent-pkg-scarab-test"),
        "{err}"
    );
    // Store-name shapes never reach the fetch path — the miss error
    // keeps the store-only contract.
    for spec in ["ghost", "ghost@9.9.9", "@scope/ghost"] {
        let err = c
            .resolve_yaml(
                &format!("params:\n  include: tamias:{spec}//frag.yaml"),
                tmp.path(),
            )
            .unwrap_err();
        assert!(err.reason().contains("store-only"), "{spec}: {err}");
        assert!(err.reason().contains("tamias install"), "{spec}: {err}");
    }
}

#[cfg(feature = "tamias")]
#[test]
fn tamias_disabled_composer_errors_loudly() {
    let tmp = tempfile::tempdir().unwrap();
    let c = composer().without_tamias();
    let err = c
        .resolve_yaml("params:\n  include: tamias:ghost//frag.yaml", tmp.path())
        .unwrap_err();
    assert!(err.reason().contains("disabled"), "{err}");
}

#[cfg(not(feature = "tamias"))]
#[test]
fn tamias_refs_error_when_feature_is_off() {
    let tmp = tempfile::tempdir().unwrap();
    let err = load("params:\n  include: tamias:ghost//frag.yaml", tmp.path()).unwrap_err();
    assert!(err.reason().contains("not compiled in"), "{err}");
}
