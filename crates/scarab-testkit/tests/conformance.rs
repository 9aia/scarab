//! Runs every conformance suite against the workspace impls —
//! `TomlFormat`/`YamlFormat` for [`assert_config_format_conformance`],
//! `FileSource`/`LayeredFileSource` for
//! [`assert_discovery_conformance`], and the pure merge fns for the
//! merge suites.

use std::collections::BTreeMap;

use scarab_config::Discovery;
use scarab_config_file::{FileSource, LayeredFileSource};
use scarab_config_toml::TomlFormat;
use scarab_config_yaml::YamlFormat;
use scarab_testkit::{
    DiscoveryCase, FormatCase, MergeCase, OverrideCase, PreparedCase, ServerConfig, TestConfig,
    assert_apply_overrides_conformance, assert_config_format_conformance,
    assert_config_source_conformance, assert_config_source_error, assert_discovery_conformance,
    assert_merge_value_conformance, fixture_text,
};
use serde_json::json;

fn fixture_app() -> TestConfig {
    TestConfig {
        name: "fixture-app".into(),
        port: 8080,
        tags: vec!["alpha".into(), "beta".into()],
        server: ServerConfig {
            host: "0.0.0.0".into(),
            tls: true,
        },
    }
}

fn file_source(pc: &PreparedCase) -> FileSource<TestConfig> {
    let mut src = FileSource::new(
        Discovery::bare("myapp")
            .dirs(pc.dirs().to_vec())
            .env_var("MYAPP_CONFIG"),
    );
    src.explicit = pc.explicit().map(|p| p.to_string_lossy().into_owned());
    src
}

fn layered_source(pc: &PreparedCase) -> LayeredFileSource<TestConfig> {
    let mut src = LayeredFileSource::new(
        Discovery::bare("myapp")
            .dirs(pc.dirs().to_vec())
            .env_var("MYAPP_CONFIG"),
    );
    src.explicit = pc.explicit().map(|p| p.to_string_lossy().into_owned());
    src
}

#[test]
fn yaml_format_conformance() {
    assert_config_format_conformance(
        &YamlFormat,
        &FormatCase::new("app.yaml", fixture_text("app.yaml"), fixture_app()).bad([
            "name: [unclosed".to_string(),
            "port: \"not-a-number\"".to_string(),
        ]),
    );
}

#[test]
fn toml_format_conformance() {
    assert_config_format_conformance(
        &TomlFormat,
        &FormatCase::new("app.toml", fixture_text("app.toml"), fixture_app()).bad([
            "name = [unclosed".to_string(),
            "port = \"not-a-number\"".to_string(),
        ]),
    );
}

#[test]
fn file_source_yaml_wins_over_toml() {
    let expected = TestConfig {
        name: "yaml".into(),
        port: 2,
        ..TestConfig::default()
    };
    assert_discovery_conformance(
        &DiscoveryCase::loaded("yaml-wins", expected)
            .file("d/myapp.yaml", "name: yaml\nport: 2\n")
            .file("d/myapp.toml", "name = \"toml\"\nport: 1\n")
            .dir("d"),
        file_source,
    );
}

#[test]
fn file_source_yml_fills_when_yaml_absent() {
    let expected = TestConfig {
        name: "yml".into(),
        ..TestConfig::default()
    };
    assert_discovery_conformance(
        &DiscoveryCase::loaded("yml-fills", expected)
            .file("d/myapp.yml", "name: yml\n")
            .file("d/myapp.toml", "name = \"toml\"\n")
            .dir("d"),
        file_source,
    );
}

#[test]
fn file_source_earlier_dir_wins() {
    let expected = TestConfig {
        name: "first".into(),
        ..TestConfig::default()
    };
    assert_discovery_conformance(
        &DiscoveryCase::loaded("dir-order", expected)
            .file("a/myapp.toml", "name = \"first\"\n")
            .file("b/myapp.yaml", "name: second\n")
            .dir("a")
            .dir("b"),
        file_source,
    );
}

#[test]
fn file_source_explicit_path_wins() {
    let expected = TestConfig {
        name: "explicit".into(),
        ..TestConfig::default()
    };
    assert_discovery_conformance(
        &DiscoveryCase::loaded("explicit", expected)
            .file("d/myapp.yaml", "name: discovered\n")
            .file("custom.toml", "name = \"explicit\"\n")
            .dir("d")
            .explicit("custom.toml"),
        file_source,
    );
}

#[test]
fn file_source_missing_explicit_is_error() {
    assert_discovery_conformance(
        &DiscoveryCase::<TestConfig>::error("explicit-missing", "config file not found")
            .file("d/myapp.yaml", "name: x\n")
            .dir("d")
            .explicit("gone.yaml"),
        file_source,
    );
}

#[test]
fn file_source_env_var_path() {
    let expected = TestConfig {
        name: "env".into(),
        ..TestConfig::default()
    };
    assert_discovery_conformance(
        &DiscoveryCase::loaded("env-path", expected)
            .file("d/myapp.yaml", "name: dir\n")
            .file("env.toml", "name = \"env\"\n")
            .dir("d")
            .env_path("MYAPP_CONFIG", "env.toml"),
        file_source,
    );
}

#[test]
fn file_source_nothing_found_errors() {
    assert_discovery_conformance(
        &DiscoveryCase::<TestConfig>::error("not-found", "no myapp.").dir("empty"),
        file_source,
    );
}

#[test]
fn layered_source_merges_fixtures_low_to_high() {
    let expected = TestConfig {
        name: "user-base".into(), // only the base layer sets name
        port: 3,                  // explicit top layer wins
        tags: vec!["c".into()],   // arrays replace, never concat
        server: ServerConfig {
            host: "user-host".into(), // untouched base leaf survives
            tls: true,                // project layer wins nested leaf
        },
    };
    assert_discovery_conformance(
        &DiscoveryCase::loaded("layered-merge", expected)
            .file(
                "layered/user/myapp.yaml",
                fixture_text("layered/user/myapp.yaml"),
            )
            .file(
                "layered/project/myapp.yaml",
                fixture_text("layered/project/myapp.yaml"),
            )
            .file(
                "layered/extra/override.yaml",
                fixture_text("layered/extra/override.yaml"),
            )
            .dir("layered/project")
            .dir("layered/user")
            .explicit("layered/extra/override.yaml"),
        layered_source,
    );
}

#[test]
fn layered_source_mixed_formats_across_dirs() {
    let expected = TestConfig {
        name: "toml-base".into(),
        port: 9,
        ..TestConfig::default()
    };
    assert_discovery_conformance(
        &DiscoveryCase::loaded("mixed-formats", expected)
            .file(
                "layered/tomlbase/myapp.toml",
                fixture_text("layered/tomlbase/myapp.toml"),
            )
            .file(
                "layered/yamlover/myapp.yaml",
                fixture_text("layered/yamlover/myapp.yaml"),
            )
            .dir("layered/yamlover")
            .dir("layered/tomlbase"),
        layered_source,
    );
}

#[test]
fn layered_source_env_file_is_top_layer() {
    let expected = TestConfig {
        name: "user-base".into(),
        port: 5, // env file on top of the dir layer
        tags: vec!["a".into(), "b".into()],
        server: ServerConfig {
            host: "user-host".into(),
            tls: false,
        },
    };
    assert_discovery_conformance(
        &DiscoveryCase::loaded("env-top-layer", expected)
            .file(
                "layered/user/myapp.yaml",
                fixture_text("layered/user/myapp.yaml"),
            )
            .file(
                "layered/envtop/env.yaml",
                fixture_text("layered/envtop/env.yaml"),
            )
            .dir("layered/user")
            .env_path("MYAPP_CONFIG", "layered/envtop/env.yaml"),
        layered_source,
    );
}

#[test]
fn layered_source_overrides_are_highest_layer() {
    let expected = TestConfig {
        name: "user-base".into(),
        port: 42,
        tags: vec!["a".into(), "b".into()],
        server: ServerConfig {
            host: "user-host".into(),
            tls: true,
        },
    };
    let overrides = BTreeMap::from([
        ("port".to_string(), json!(42)),
        ("server.tls".to_string(), json!(true)),
    ]);
    assert_discovery_conformance(
        &DiscoveryCase::loaded("overrides", expected)
            .file(
                "layered/user/myapp.yaml",
                fixture_text("layered/user/myapp.yaml"),
            )
            .dir("layered/user"),
        move |pc| {
            let mut src = layered_source(pc);
            src.overrides = overrides;
            src
        },
    );
}

#[test]
fn layered_source_nothing_found_errors() {
    assert_discovery_conformance(
        &DiscoveryCase::<TestConfig>::error("layered-not-found", "no myapp.").dir("empty"),
        layered_source,
    );
}

#[test]
fn layered_source_missing_explicit_is_error() {
    assert_discovery_conformance(
        &DiscoveryCase::<TestConfig>::error("layered-explicit-missing", "config file not found")
            .file(
                "layered/user/myapp.yaml",
                fixture_text("layered/user/myapp.yaml"),
            )
            .dir("layered/user")
            .explicit("gone.yaml"),
        layered_source,
    );
}

#[test]
fn config_source_minimal_contract() {
    // assert_config_source_* need no injected env: point `explicit`
    // at an absolute fixture path so `load()` resolves directly.
    let mut src = FileSource::new(Discovery::bare("myapp"));
    src.explicit = Some(
        scarab_testkit::fixture_path("app.yaml")
            .to_string_lossy()
            .into_owned(),
    );
    assert_config_source_conformance(&src, &fixture_app());

    let mut missing = FileSource::<TestConfig>::new(Discovery::bare("myapp"));
    missing.explicit = Some("/definitely/not/here.yaml".to_string());
    assert_config_source_error(&missing, "config file not found");
}

#[test]
fn merge_value_conformance_cases() {
    assert_merge_value_conformance(&[
        MergeCase {
            name: "objects-merge-key-wise".into(),
            base: json!({"a": 1, "b": {"x": 1, "y": 2}, "c": [1, 2]}),
            overlays: vec![json!({"b": {"y": 3, "z": 4}, "c": [9]})],
            expected: json!({"a": 1, "b": {"x": 1, "y": 3, "z": 4}, "c": [9]}),
        },
        MergeCase {
            name: "arrays-replace".into(),
            base: json!({"tags": ["a", "b"]}),
            overlays: vec![json!({"tags": ["c"]}), json!({"tags": ["d", "e"]})],
            expected: json!({"tags": ["d", "e"]}),
        },
        MergeCase {
            name: "scalar-and-null-replace".into(),
            base: json!({"n": 1, "s": "x", "nil": {"k": 1}}),
            overlays: vec![json!({"n": "str", "nil": null})],
            expected: json!({"n": "str", "s": "x", "nil": null}),
        },
    ]);
}

#[test]
fn apply_overrides_conformance_cases() {
    assert_apply_overrides_conformance(&[OverrideCase {
        name: "dotted-leaves".into(),
        base: json!({"server": {"host": "h"}, "top": 1}),
        overrides: BTreeMap::from([
            ("server.port".to_string(), json!(8080)),
            ("server.tls.enabled".to_string(), json!(true)),
            ("brand.new.deep".to_string(), json!("x")),
        ]),
        expected: json!({
            "server": {"host": "h", "port": 8080, "tls": {"enabled": true}},
            "top": 1,
            "brand": {"new": {"deep": "x"}}
        }),
    }]);
}
