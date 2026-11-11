//! Dogfood tests — scarab's *own* repo config files loaded back
//! through `scarab-config-file`. Each org tool (`myco`, `sturn`,
//! `kuda`, `mimus`) reads its `<name>.toml` with its own parser;
//! these tests prove the scarab-config stack discovers, dispatches,
//! and parses the same real files — generic `serde_json::Value`
//! layers plus one typed model where the shape is stable.
//!
//! Paths resolve from `CARGO_MANIFEST_DIR` (`crates/scarab-config-file`
//! → repo root is `../..`), never the test CWD.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use scarab_config::Discovery;
use scarab_config_file::{FileSource, LayeredFileSource};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Repo root — `crates/scarab-config-file` → `../..`.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Discovery pinned at the repo root — `<name>.toml` must exist there;
/// no env step, no CWD dependence.
fn root_discovery(name: &str) -> Discovery {
    Discovery::bare(name).dirs([repo_root()])
}

/// FileSource over `T` searching only the repo root.
fn root_source<T>(name: &str) -> FileSource<T>
where
    T: serde::de::DeserializeOwned + Serialize,
{
    FileSource::new(root_discovery(name))
}

/// The typed slice of `sturn.toml` — `[repo] dir` is its whole shape.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct SturnConfig {
    repo: Repo,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Repo {
    dir: String,
}

#[test]
fn sturn_toml_loads_as_typed_model() {
    let src = root_source::<SturnConfig>("sturn");
    let cfg = src.load_with(&|_| None).unwrap();
    assert_eq!(cfg.repo.dir, ".");
    // discovery picked the .toml — no sturn.yaml exists to prefer
    assert_eq!(
        src.locate_with(&|_| None).unwrap(),
        repo_root().join("sturn.toml")
    );
}

#[test]
fn kuda_toml_loads_as_value() {
    let src = root_source::<Value>("kuda");
    let cfg = src.load_with(&|_| None).unwrap();
    let obj = cfg.as_object().unwrap();
    assert_eq!(obj["store"]["path"].as_str().unwrap(), ".kuda/index.sqlite");
    assert_eq!(obj["extract"]["kinds"].as_array().unwrap().len(), 2);
    assert_eq!(obj["cards"]["dir"].as_str().unwrap(), ".kuda");
}

#[test]
fn mimus_toml_loads_as_value() {
    let src = root_source::<Value>("mimus");
    let cfg = src.load_with(&|_| None).unwrap();
    let obj = cfg.as_object().unwrap();
    assert!(obj["check"]["mechanical"].as_bool().unwrap());
    assert_eq!(obj["report"]["format"].as_str().unwrap(), "human");
    assert_eq!(obj["sources"]["docs"].as_array().unwrap().len(), 3);
}

#[test]
fn myco_toml_is_all_comments_and_still_parses() {
    // myco.toml ships as a fully-commented prelude template — TOML
    // legal, parses to an empty table.
    let src = root_source::<Value>("myco");
    let cfg = src.load_with(&|_| None).unwrap();
    assert!(cfg.is_object());
}

#[test]
fn cargo_config_toml_loads_via_explicit_path() {
    // `.cargo/config.toml` — outside the `<name>.<ext>` discovery
    // shape, reached through the explicit-path seam.
    let mut src = root_source::<Value>("cargo");
    src.explicit = Some(
        repo_root()
            .join(".cargo/config.toml")
            .to_string_lossy()
            .into_owned(),
    );
    let cfg = src.load_with(&|_| None).unwrap();
    let obj = cfg.as_object().unwrap();
    assert_eq!(
        obj["alias"]["xtask"].as_str().unwrap(),
        "run --package scarab-xtask --"
    );
}

#[test]
fn layered_source_merges_every_repo_config() {
    // One dir layer per `<name>.toml` — the merge path sees the same
    // files FileSource resolves first-match.
    for name in ["sturn", "kuda", "mimus", "myco"] {
        let src = LayeredFileSource::<Value>::new(root_discovery(name));
        let layers = src.layer_paths_with(&|_| None).unwrap();
        assert_eq!(layers, vec![repo_root().join(format!("{name}.toml"))]);
        let cfg = src.load_with(&|_| None).unwrap();
        assert!(cfg.is_object(), "{name}.toml did not merge to an object");
    }
}
