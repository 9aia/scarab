//! Document-level `extends:` tests — COMPOSITION.md chain semantics
//! over `serde_json::Value` documents.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

#[cfg(feature = "tamias")]
use scarab_compose::TamiasRoots;
use scarab_compose::{ComposeError, Composer};
use serde_json::{Value, json};

fn composer() -> Composer {
    Composer::new()
}

fn load(text: &str, dir: &Path) -> Result<Value, ComposeError> {
    composer().resolve_yaml(text, dir)
}

#[test]
fn single_string_parent_merges_with_child_winning() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("base.yaml"),
        "name: base\ntimeout: 300\nnested:\n  a: 1\n  b: 1\nlist: [base]\n",
    )
    .unwrap();
    let doc = load(
        "extends: base.yaml\ntimeout: 42\nnested:\n  b: 2\nlist: [child]\n",
        tmp.path(),
    )
    .unwrap();
    assert_eq!(doc["name"], json!("base"));
    assert_eq!(doc["timeout"], 42);
    // Dicts merge per-key; arrays concat child-first… (deep_merge:
    // local items first, then imported).
    assert_eq!(doc["nested"], json!({"a": 1, "b": 2}));
    assert_eq!(doc["list"], json!(["child", "base"]));
    assert!(doc.get("extends").is_none());
}

#[test]
fn extends_array_is_lowest_precedence_first() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("low.yaml"), "who: low\nshared: low\n").unwrap();
    fs::write(tmp.path().join("high.yaml"), "who: high\n").unwrap();
    // Declaration order = render order: low first, high wins merges.
    let doc = load("extends: [low.yaml, high.yaml]\n", tmp.path()).unwrap();
    assert_eq!(doc["who"], json!("high"));
    assert_eq!(doc["shared"], json!("low"));
    // Reversed order flips the winner.
    let doc = load("extends: [high.yaml, low.yaml]\n", tmp.path()).unwrap();
    assert_eq!(doc["who"], json!("low"));
}

#[test]
fn nested_chain_composes_grandparent_parent_child() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("grand.yaml"), "g: 1\nwho: grand\n").unwrap();
    fs::write(
        tmp.path().join("parent.yaml"),
        "extends: grand.yaml\np: 1\nwho: parent\n",
    )
    .unwrap();
    let doc = load("extends: parent.yaml\nc: 1\n", tmp.path()).unwrap();
    assert_eq!(doc["g"], 1);
    assert_eq!(doc["p"], 1);
    assert_eq!(doc["c"], 1);
    assert_eq!(doc["who"], json!("parent"));
    assert!(doc.get("extends").is_none());
}

#[test]
fn child_beats_all_ancestors() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("base.yaml"), "who: base\n").unwrap();
    let doc = load("extends: base.yaml\nwho: child\n", tmp.path()).unwrap();
    assert_eq!(doc["who"], json!("child"));
}

#[test]
fn diamond_ancestor_composes_once_at_first_occurrence() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("shared.yaml"),
        "from: shared\nwho: shared\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("p1.yaml"),
        "extends: shared.yaml\np1: true\nwho: p1\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("p2.yaml"),
        "extends: shared.yaml\np2: true\nwho: p2\n",
    )
    .unwrap();
    let doc = load("extends: [p1.yaml, p2.yaml]\n", tmp.path()).unwrap();
    // Shared ancestor contributed once; p2 (later parent) wins `who`.
    assert_eq!(doc["from"], json!("shared"));
    assert_eq!(doc["p1"], json!(true));
    assert_eq!(doc["p2"], json!(true));
    assert_eq!(doc["who"], json!("p2"));
}

#[test]
fn duplicate_parent_in_one_list_collapses() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.yaml"), "v: a\nitems: [a]\n").unwrap();
    let doc = load("extends: [a.yaml, a.yaml]\n", tmp.path()).unwrap();
    // Second occurrence skipped — arrays don't double-append.
    assert_eq!(doc["items"], json!(["a"]));
}

#[test]
fn detects_circular_extends() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.yaml"), "extends: b.yaml\na: 1\n").unwrap();
    fs::write(tmp.path().join("b.yaml"), "extends: a.yaml\nb: 1\n").unwrap();
    let err = load("extends: a.yaml\n", tmp.path()).unwrap_err();
    assert!(
        err.reason().starts_with("circular extends detected:"),
        "{err}"
    );
}

#[test]
fn detects_self_extends() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("main.yaml"), "extends: main.yaml\n").unwrap();
    let err = composer()
        .resolve_file(&tmp.path().join("main.yaml"))
        .unwrap_err();
    assert!(
        err.reason().starts_with("circular extends detected:"),
        "{err}"
    );
}

#[test]
fn enforces_max_depth() {
    let tmp = tempfile::tempdir().unwrap();
    // p1..p8 chain = 8 ancestors — allowed.
    for i in 1..=8 {
        let next = if i == 8 {
            String::new()
        } else {
            format!("extends: p{}.yaml\n", i + 1)
        };
        fs::write(
            tmp.path().join(format!("p{i}.yaml")),
            format!("{next}v: {i}\n"),
        )
        .unwrap();
    }
    let doc = load("extends: p1.yaml\n", tmp.path()).unwrap();
    assert_eq!(doc["v"], 1);
    // Add a 9th ancestor — p8 now declares a parent at depth 8.
    fs::write(tmp.path().join("p9.yaml"), "v: 9\n").unwrap();
    fs::write(tmp.path().join("p8.yaml"), "extends: p9.yaml\nv: 8\n").unwrap();
    let err = load("extends: p1.yaml\n", tmp.path()).unwrap_err();
    assert!(err.reason().contains("maximum depth"), "{err}");
}

#[test]
fn parent_includes_resolve_relative_to_parent_dir() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("sub")).unwrap();
    // Decoy at the root — the parent's include must NOT pick this up.
    fs::write(tmp.path().join("frag.yaml"), "who: root\n").unwrap();
    fs::write(tmp.path().join("sub/frag.yaml"), "who: sub\n").unwrap();
    fs::write(
        tmp.path().join("sub/parent.yaml"),
        "params:\n  include: frag.yaml\n  cmd: run\n",
    )
    .unwrap();
    let doc = load("extends: sub/parent.yaml\n", tmp.path()).unwrap();
    assert_eq!(doc["params"]["who"], json!("sub"));
    assert_eq!(doc["params"]["cmd"], json!("run"));
}

#[test]
fn parent_include_expressions_anchor_at_parent_dir() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("sub")).unwrap();
    fs::write(tmp.path().join("sub/inner.yaml"), "deep: true\n").unwrap();
    fs::write(
        tmp.path().join("sub/parent.yaml"),
        "inner: ${include(inner.yaml)}\n",
    )
    .unwrap();
    let doc = load("extends: sub/parent.yaml\n", tmp.path()).unwrap();
    assert_eq!(doc["inner"]["deep"], json!(true));
}

#[test]
fn child_and_parent_both_include_and_extend() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("base.yaml"), "from: base\n").unwrap();
    fs::write(tmp.path().join("frag.yaml"), "extra: 1\n").unwrap();
    let doc = load(
        "extends: base.yaml\nparams:\n  include: frag.yaml\n",
        tmp.path(),
    )
    .unwrap();
    assert_eq!(doc["from"], json!("base"));
    assert_eq!(doc["params"]["extra"], 1);
}

#[test]
fn extends_key_consumed_and_empty_forms() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("base.yaml"), "v: 1\n").unwrap();
    // extends: [] → no parents, key still consumed.
    let doc = load("extends: []\nown: true\n", tmp.path()).unwrap();
    assert_eq!(doc["own"], json!(true));
    assert!(doc.get("extends").is_none());
    // extends: null → same.
    let doc = load("extends:\nown: true\n", tmp.path()).unwrap();
    assert!(doc.get("extends").is_none());
    // No extends → untouched (zero-cost path).
    let doc = load("plain: true\n", tmp.path()).unwrap();
    assert_eq!(doc["plain"], json!(true));
}

#[test]
fn structured_source_and_via_entries() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("base.yaml"), "v: base\n").unwrap();
    // {source} alone works; {source, via: tamias-ref} accepted.
    let doc = load("extends: {source: base.yaml}\n", tmp.path()).unwrap();
    assert_eq!(doc["v"], json!("base"));
    let doc = load(
        "extends: [{source: base.yaml, via: tamias-ref}]\n",
        tmp.path(),
    )
    .unwrap();
    assert_eq!(doc["v"], json!("base"));
    // Unknown via → error, never a silent fallback.
    let err = load("extends: {source: base.yaml, via: bogus}\n", tmp.path()).unwrap_err();
    assert!(
        err.reason().contains("unsupported extends 'via' scheme"),
        "{err}"
    );
    // Missing source → error.
    let err = load("extends: {via: tamias-ref}\n", tmp.path()).unwrap_err();
    assert!(err.reason().contains("'source'"), "{err}");
}

#[test]
fn malformed_extends_values_error() {
    let tmp = tempfile::tempdir().unwrap();
    for (text, needle) in [
        ("extends: 5\n", "extends must be"),
        ("extends: [base.yaml, 5]\n", "extends entries must be"),
        ("extends: ''\n", "must not be empty"),
        ("extends: {source: 5}\n", "'source' must be a string"),
    ] {
        let err = load(text, tmp.path()).unwrap_err();
        assert!(err.reason().contains(needle), "{text}: {err}");
    }
}

#[test]
fn missing_parent_is_a_read_error() {
    let tmp = tempfile::tempdir().unwrap();
    let err = load("extends: missing.yaml\n", tmp.path()).unwrap_err();
    assert!(err.reason().starts_with("Failed to read file:"), "{err}");
}

#[test]
fn parents_parse_by_extension() {
    let tmp = tempfile::tempdir().unwrap();
    // TOML parent — spec packs are TOML.
    fs::write(tmp.path().join("base.toml"), "v = 7\nwho = \"toml\"\n").unwrap();
    let doc = composer()
        .resolve_toml("extends = \"base.toml\"\nextra = true\n", tmp.path())
        .unwrap();
    assert_eq!(doc["v"], 7);
    assert_eq!(doc["who"], json!("toml"));
    assert_eq!(doc["extra"], json!(true));
    // JSON parent.
    fs::write(tmp.path().join("base.json"), "{\"v\": 9}").unwrap();
    let doc = load("extends: base.json\n", tmp.path()).unwrap();
    assert_eq!(doc["v"], 9);
    // A TOML parent can itself extend a YAML grandparent.
    fs::write(tmp.path().join("grand.yaml"), "g: 1\n").unwrap();
    fs::write(
        tmp.path().join("mid.toml"),
        "extends = \"grand.yaml\"\nm = 2\n",
    )
    .unwrap();
    let doc = load("extends: mid.toml\n", tmp.path()).unwrap();
    assert_eq!(doc["g"], 1);
    assert_eq!(doc["m"], 2);
}

#[test]
fn extends_inside_nested_positions_is_inert_data() {
    let tmp = tempfile::tempdir().unwrap();
    // Only the document's TOP-LEVEL extends: composes — a nested one
    // is just a key (no file is read for it).
    let doc = load("nodes:\n  - id: n\n    extends: nowhere.yaml\n", tmp.path()).unwrap();
    assert_eq!(doc["nodes"][0]["extends"], json!("nowhere.yaml"));
}

#[test]
fn snippets_compose_across_the_chain() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("base.yaml"),
        "snippets:\n  env:\n    REGION: us-east-1\nnodes: []\n",
    )
    .unwrap();
    let doc = load(
        "extends: base.yaml\nnodes:\n  - id: n\n    params:\n      env: ${snippet(env)}\n",
        tmp.path(),
    )
    .unwrap();
    assert_eq!(
        doc["nodes"][0]["params"]["env"],
        json!({"REGION": "us-east-1"})
    );
}

#[test]
fn resolve_file_honors_extends_relative_to_file_dir() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("sub")).unwrap();
    fs::write(tmp.path().join("sub/base.yaml"), "v: from-sub\n").unwrap();
    fs::write(tmp.path().join("sub/child.yaml"), "extends: base.yaml\n").unwrap();
    let doc = composer()
        .resolve_file(&tmp.path().join("sub/child.yaml"))
        .unwrap();
    assert_eq!(doc["v"], json!("from-sub"));
}

// --- tamias: extends parents ----------------------------------------------

/// Write a fake store entry — same shape as tests/compose.rs.
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
#[test]
fn tamias_extends_parent_resolves_from_store() {
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
        &[("docs/base.yaml", "timeout: 42\nwho: bundle\n")],
    );
    let c = Composer::with_tamias_roots(TamiasRoots::new(&store, &cache));
    let doc = c
        .resolve_yaml(
            "extends: tamias:handl-gates//docs/base.yaml\nwho: local\n",
            tmp.path(),
        )
        .unwrap();
    assert_eq!(doc["timeout"], 42);
    assert_eq!(doc["who"], json!("local"));
}

#[cfg(feature = "tamias")]
#[test]
fn tamias_via_hint_and_disabled_composer() {
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
        &[("base.yaml", "v: 1\n")],
    );
    let c = Composer::with_tamias_roots(TamiasRoots::new(&store, &cache));
    let doc = c
        .resolve_yaml(
            "extends: [{source: \"tamias:handl-gates//base.yaml\", via: tamias-ref}]\n",
            tmp.path(),
        )
        .unwrap();
    assert_eq!(doc["v"], 1);
    // without_tamias: the same ref errors loudly.
    let err = composer()
        .without_tamias()
        .resolve_yaml("extends: tamias:handl-gates//base.yaml\n", tmp.path())
        .unwrap_err();
    assert!(err.reason().contains("disabled"), "{err}");
}

#[cfg(not(feature = "tamias"))]
#[test]
fn tamias_extends_error_when_feature_is_off() {
    let tmp = tempfile::tempdir().unwrap();
    let err = load("extends: tamias:ghost//base.yaml\n", tmp.path()).unwrap_err();
    assert!(err.reason().contains("not compiled in"), "{err}");
}
