//! Layered file config source — the merge counterpart of
//! [`FileSource`]. Where `FileSource` picks the *first* existing
//! config file, `LayeredFileSource` collects **one file per search
//! dir** (same per-dir preference rules: formats in order, extensions
//! in declared order — YAML before TOML), merges them low→high
//! precedence into a single `serde_json::Value` tree, applies
//! programmatic dotted-path overrides, then deserializes into `T`.
//!
//! Precedence, lowest to highest:
//!
//! 1. `<dir>/<name>.<ext>` per dir, **last dir first** — under
//!    [`Discovery::for_app`] conventions (`[., ~/.config/<name>]`)
//!    the user config is the base and `./<name>.yaml` overrides it.
//! 2. The explicit path / `$<NAME>_CONFIG` file — the *top* file
//!    layer, merged over every dir layer (unlike `FileSource`, where
//!    it replaces discovery entirely).
//! 3. [`overrides`](LayeredFileSource::overrides) — flat
//!    dotted-path leaf values, the highest layer.
//!
//! Merge semantics live in [`scarab_config::merge_value`]: JSON
//! objects merge key-wise recursively; scalars and arrays replace.
//! Each file is parsed as `serde_json::Value` (the `-toml`/`-yaml`
//! blanket impls serve `Value`), so `T` itself never needs
//! `Serialize` — only `DeserializeOwned` for the final
//! `serde_json::from_value`.
//!
//! [`FileSource`]: crate::FileSource
//! [`Discovery::for_app`]: scarab_config::Discovery::for_app

use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use scarab_config::{
    ConfigFormat, ConfigSource, Discovery, apply_overrides, expand_tilde, merge_value,
};
use scarab_config_toml::TomlFormat;
use scarab_config_yaml::YamlFormat;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// Layered file-backed config source — every applicable file merged
/// into one `T`. Formats are held as `ConfigFormat<serde_json::Value>`
/// so layers merge as JSON before the typed deserialize.
///
/// [`ConfigFormat`]: scarab_config::ConfigFormat
pub struct LayeredFileSource<T> {
    /// Where to search — app basename, env var, dirs.
    pub discovery: Discovery,
    /// Explicit `--config`-style path; the top file layer (merged over
    /// all dir layers). Wins over `discovery.env_var`.
    pub explicit: Option<String>,
    /// Syntax formats in preference order, instantiated over
    /// `serde_json::Value` — earlier formats win candidate ties within
    /// a directory.
    pub formats: Vec<Box<dyn ConfigFormat<Value>>>,
    /// Programmatic highest-precedence layer — flat dotted-path keys
    /// (`"server.port"`) → leaf values, applied after all file layers.
    pub overrides: BTreeMap<String, Value>,
    /// The target model `T` — layers merge as `serde_json::Value`, so
    /// `T` only appears at the final `from_value`.
    marker: PhantomData<fn() -> T>,
}

impl<T> LayeredFileSource<T> {
    /// A layered source over `discovery` with caller-chosen Value
    /// formats (any order — earlier formats win candidate ties).
    #[must_use]
    pub fn with_formats(discovery: Discovery, formats: Vec<Box<dyn ConfigFormat<Value>>>) -> Self {
        Self {
            discovery,
            explicit: None,
            formats,
            overrides: BTreeMap::new(),
            marker: PhantomData,
        }
    }

    /// The org-standard format pair over `serde_json::Value` — YAML
    /// preferred, TOML fallback.
    #[must_use]
    pub fn standard_formats() -> Vec<Box<dyn ConfigFormat<Value>>> {
        vec![Box::new(YamlFormat), Box::new(TomlFormat)]
    }

    /// Every layer path in **merge order — lowest precedence first**:
    /// last search dir → first search dir → explicit/env file.
    /// `env` supplies env-var lookups so callers/tests never have to
    /// touch the process environment.
    ///
    /// # Errors
    /// Fails when an explicit/env path is set but missing, or when no
    /// layer file exists anywhere.
    pub fn layer_paths_with(
        &self,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Vec<PathBuf>, String> {
        let explicit = self
            .explicit
            .clone()
            .or_else(|| self.env_path(env))
            .filter(|p| !p.is_empty())
            .map(PathBuf::from);
        if let Some(path) = &explicit {
            if !path.is_file() {
                return Err(format!("config file not found: {}", path.display()));
            }
        }

        let name = &self.discovery.name;
        let mut layers = Vec::new();
        // last dir = lowest precedence = merged first
        for dir in self.discovery.dirs.iter().rev() {
            let dir = expand_tilde(dir);
            if let Some(path) = self.preferred_in(&dir, name) {
                layers.push(path);
            }
        }
        if let Some(path) = explicit {
            layers.push(path);
        }
        if layers.is_empty() {
            return Err(self.not_found_error());
        }
        Ok(layers)
    }

    /// The layer paths using the process environment.
    ///
    /// # Errors
    /// Same as [`layer_paths_with`](Self::layer_paths_with).
    pub fn layer_paths(&self) -> Result<Vec<PathBuf>, String> {
        self.layer_paths_with(&|k| std::env::var(k).ok())
    }

    /// Parse one layer file into a `serde_json::Value`. Extension
    /// dispatch mirrors `FileSource::load_path`: an extensionless path
    /// parses with the first (preferred) format; an unclaimed
    /// extension is an error; a non-mapping document is an error (a
    /// layer must be a table to merge).
    ///
    /// # Errors
    /// Fails on a read error, an unclaimed extension, a non-mapping
    /// document, or a parse error — all naming the path.
    pub fn parse_layer(&self, path: &Path) -> Result<Value, String> {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let format = self.format_for_path(path).ok_or_else(|| {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("?");
            format!("no config format claims '.{ext}': {}", path.display())
        })?;
        let value = format
            .parse(&text)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if !matches!(value, Value::Object(_)) {
            return Err(format!("{}: config layer is not a mapping", path.display()));
        }
        Ok(value)
    }

    /// The env-var path, if `discovery.env_var` is set and non-empty.
    fn env_path(&self, env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
        self.discovery
            .env_var
            .as_deref()
            .and_then(env)
            .filter(|v| !v.is_empty())
    }

    /// The preferred existing `<dir>/<name>.<ext>` — formats in order,
    /// each format's extensions in declared order; first hit wins.
    fn preferred_in(&self, dir: &Path, name: &str) -> Option<PathBuf> {
        for format in &self.formats {
            for ext in format.extensions() {
                let path = dir.join(format!("{name}.{ext}"));
                if path.is_file() {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Format for `path`: extension match (same rule as
    /// `scarab_config::format_for`, inlined so the borrow ties to
    /// `self`); an extensionless path falls back to the first
    /// (preferred) format.
    fn format_for_path<'a>(&'a self, path: &Path) -> Option<&'a dyn ConfigFormat<Value>> {
        match path.extension().and_then(|e| e.to_str()) {
            None => self.formats.first().map(|f| &**f),
            Some(ext) => self
                .formats
                .iter()
                .find(|f| f.extensions().contains(&ext))
                .map(|f| &**f),
        }
    }

    /// The "nothing found" error — same shape as `FileSource`'s:
    /// names the app, claimed extensions, and search steps.
    fn not_found_error(&self) -> String {
        let exts: Vec<&str> = self
            .formats
            .iter()
            .flat_map(|f| f.extensions().iter().copied())
            .collect();
        let mut searched: Vec<String> = Vec::new();
        if let Some(var) = &self.discovery.env_var {
            searched.push(format!("${var}"));
        }
        for dir in &self.discovery.dirs {
            searched.push(expand_tilde(dir).display().to_string());
        }
        searched.push("…".to_string());
        format!(
            "no {}.{{{}}} found (search: {})",
            self.discovery.name,
            exts.join(","),
            searched.join(", ")
        )
    }
}

impl<T> LayeredFileSource<T>
where
    T: DeserializeOwned,
{
    /// A source for app `name` — [`Discovery::for_app`] conventions +
    /// the standard formats.
    #[must_use]
    pub fn new(discovery: Discovery) -> Self {
        Self::with_formats(discovery, Self::standard_formats())
    }

    /// A layered source for app `name` — [`Discovery::for_app`]
    /// conventions + the standard formats.
    #[must_use]
    pub fn for_app(name: impl Into<String>) -> Self {
        Self::new(Discovery::for_app(name))
    }

    /// Load + merge every layer, then deserialize into `T`; `env`
    /// supplies env-var lookups.
    ///
    /// # Errors
    /// Fails on a missing explicit path, no files found, a read or
    /// parse error naming its path, a non-mapping layer, or a merged
    /// tree that doesn't fit `T`.
    pub fn load_with(&self, env: &dyn Fn(&str) -> Option<String>) -> Result<T, String> {
        let mut merged = Value::Object(Map::new());
        for path in self.layer_paths_with(env)? {
            merge_value(&mut merged, self.parse_layer(&path)?);
        }
        apply_overrides(&mut merged, &self.overrides);
        serde_json::from_value(merged).map_err(|e| format!("merged config: {e}"))
    }

    /// Load + merge every layer using the process environment.
    ///
    /// # Errors
    /// Same as [`load_with`](Self::load_with).
    pub fn load(&self) -> Result<T, String> {
        self.load_with(&|k| std::env::var(k).ok())
    }
}

impl<T> ConfigSource<T> for LayeredFileSource<T>
where
    T: DeserializeOwned,
{
    fn load(&self) -> Result<T, String> {
        LayeredFileSource::load(self)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde::Serialize;

    #[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(default)]
    struct Cfg {
        name: String,
        port: u16,
        server: Server,
        tags: Vec<String>,
    }

    #[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(default)]
    struct Server {
        host: String,
        tls: bool,
    }

    fn layered_in(dirs: &[&Path]) -> LayeredFileSource<Cfg> {
        LayeredFileSource::new(
            Discovery::bare("myapp")
                .dirs(dirs.iter().map(|d| d.to_path_buf()))
                .env_var("MYAPP_CONFIG"),
        )
    }

    #[test]
    fn project_dir_overrides_user_dir() {
        let user = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        // dirs order: project first, user second — last dir is the base
        std::fs::write(
            user.path().join("myapp.yaml"),
            "name: user\nport: 1\nserver:\n  host: user-host\n  tls: false\ntags: [a]\n",
        )
        .unwrap();
        std::fs::write(
            project.path().join("myapp.yaml"),
            "port: 2\nserver:\n  tls: true\n",
        )
        .unwrap();
        let src = layered_in(&[project.path(), user.path()]);
        let cfg = src.load_with(&|_| None).unwrap();
        assert_eq!(cfg.name, "user"); // untouched key survives from base
        assert_eq!(cfg.port, 2); // project wins
        assert_eq!(cfg.server.host, "user-host"); // nested merge keeps base leaf
        assert!(cfg.server.tls); // project wins nested leaf
        assert_eq!(cfg.tags, vec!["a".to_string()]);
    }

    #[test]
    fn arrays_replace_not_concat() {
        let user = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::write(user.path().join("myapp.yaml"), "tags: [a, b]\n").unwrap();
        std::fs::write(project.path().join("myapp.yaml"), "tags: [c]\n").unwrap();
        let src = layered_in(&[project.path(), user.path()]);
        let cfg = src.load_with(&|_| None).unwrap();
        assert_eq!(cfg.tags, vec!["c".to_string()]);
    }

    #[test]
    fn mixed_formats_across_dirs() {
        let user = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::write(
            user.path().join("myapp.toml"),
            "name = \"toml-user\"\nport = 7\n",
        )
        .unwrap();
        std::fs::write(project.path().join("myapp.yaml"), "port: 9\n").unwrap();
        let src = layered_in(&[project.path(), user.path()]);
        let cfg = src.load_with(&|_| None).unwrap();
        assert_eq!(cfg.name, "toml-user");
        assert_eq!(cfg.port, 9);
    }

    #[test]
    fn explicit_path_is_top_layer_not_replacement() {
        let user = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let extra = tempfile::tempdir().unwrap();
        std::fs::write(user.path().join("myapp.yaml"), "name: user\nport: 1\n").unwrap();
        std::fs::write(project.path().join("myapp.yaml"), "port: 2\n").unwrap();
        let p = extra.path().join("override.yaml");
        std::fs::write(&p, "port: 3\n").unwrap();
        let mut src = layered_in(&[project.path(), user.path()]);
        src.explicit = Some(p.to_string_lossy().into_owned());
        let cfg = src.load_with(&|_| None).unwrap();
        assert_eq!(cfg.name, "user"); // dir layers still merged
        assert_eq!(cfg.port, 3); // explicit on top
    }

    #[test]
    fn explicit_missing_is_hard_error_even_with_dir_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("myapp.yaml"), "name: x\n").unwrap();
        let mut src = layered_in(&[dir.path()]);
        src.explicit = Some(dir.path().join("gone.yaml").to_string_lossy().into_owned());
        let err = src.load_with(&|_| None).unwrap_err();
        assert!(err.contains("config file not found"), "{err}");
    }

    #[test]
    fn env_path_is_top_layer() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("myapp.yaml"), "name: base\nport: 1\n").unwrap();
        let p = dir.path().join("env.yaml");
        std::fs::write(&p, "port: 5\n").unwrap();
        let src = layered_in(&[dir.path()]);
        let env = |k: &str| (k == "MYAPP_CONFIG").then(|| p.to_string_lossy().into_owned());
        let layers = src.layer_paths_with(&env).unwrap();
        assert_eq!(layers.last().unwrap(), &p);
        let cfg = src.load_with(&env).unwrap();
        assert_eq!(cfg.name, "base");
        assert_eq!(cfg.port, 5);
    }

    #[test]
    fn overrides_are_highest_layer() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("myapp.yaml"),
            "name: file\nport: 1\nserver:\n  host: h\n",
        )
        .unwrap();
        let mut src = layered_in(&[dir.path()]);
        src.overrides = BTreeMap::from([
            ("port".to_string(), serde_json::json!(42)),
            ("server.tls".to_string(), serde_json::json!(true)),
            ("extra.deep".to_string(), serde_json::json!("new")),
        ]);
        let cfg = src.load_with(&|_| None).unwrap();
        assert_eq!(cfg.port, 42);
        assert_eq!(cfg.server.host, "h");
        assert!(cfg.server.tls);
        // unknown keys land in the merged JSON but are ignored by serde
        // unless the model captures them
    }

    #[test]
    fn missing_everything_errors_like_file_source() {
        let empty = tempfile::tempdir().unwrap();
        let src = layered_in(&[empty.path()]);
        let err = src.load_with(&|_| None).unwrap_err();
        assert!(err.contains("no myapp."), "{err}");
        assert!(err.contains("MYAPP_CONFIG"), "{err}");
    }

    #[test]
    fn unclaimed_extension_on_explicit_errors() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("cfg.json");
        std::fs::write(&p, "{}").unwrap();
        let mut src = layered_in(&[dir.path()]);
        src.explicit = Some(p.to_string_lossy().into_owned());
        let err = src.load_with(&|_| None).unwrap_err();
        assert!(err.contains("no config format claims"), "{err}");
    }

    #[test]
    fn parse_error_names_the_path() {
        let user = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::write(user.path().join("myapp.yaml"), "name: ok\n").unwrap();
        std::fs::write(project.path().join("myapp.yaml"), "name: [unclosed\n").unwrap();
        let src = layered_in(&[project.path(), user.path()]);
        let err = src.load_with(&|_| None).unwrap_err();
        assert!(err.contains("myapp.yaml"), "{err}");
    }

    #[test]
    fn yaml_preferred_within_a_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("myapp.toml"), "name = \"toml\"\n").unwrap();
        std::fs::write(dir.path().join("myapp.yml"), "name: yml\n").unwrap();
        std::fs::write(dir.path().join("myapp.yaml"), "name: yaml\n").unwrap();
        let src = layered_in(&[dir.path()]);
        let cfg = src.load_with(&|_| None).unwrap();
        assert_eq!(cfg.name, "yaml");
        // only one layer: the preferred file
        assert_eq!(src.layer_paths_with(&|_| None).unwrap().len(), 1);
    }

    #[test]
    fn merged_tree_must_fit_model() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("myapp.yaml"), "name: x\nport: notanumber\n").unwrap();
        let src = layered_in(&[dir.path()]);
        let err = src.load_with(&|_| None).unwrap_err();
        assert!(err.contains("merged config"), "{err}");
    }

    #[test]
    fn config_source_trait_impl() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("myapp.yaml"), "name: trait\n").unwrap();
        let src = layered_in(&[dir.path()]);
        let cfg = ConfigSource::<Cfg>::load(&src).unwrap();
        assert_eq!(cfg.name, "trait");
    }
}
