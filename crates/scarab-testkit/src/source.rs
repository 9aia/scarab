//! Source-level conformance — the [`ConfigSource`] contract plus a
//! scripted-discovery runner for file-backed sources.
//!
//! [`assert_config_source_conformance`] /
//! [`assert_config_source_error`] cover any `ConfigSource<T>` whose
//! `load()` is drivable as-is. [`assert_discovery_conformance`] goes
//! further for sources that accept an injected env lookup (the
//! [`EnvAwareSource`] trait — implemented here for
//! [`FileSource`] and [`LayeredFileSource`]): it materializes a
//! [`DiscoveryCase`] into a temp dir, builds the env map, hands the
//! prepared case to a factory closure that constructs the source, and
//! asserts the load outcome — so explicit-path-wins, env fallback,
//! first-match-vs-merge, and missing-file errors are all checked
//! without touching the process environment.
//!
//! [`FileSource`]: scarab_config_file::FileSource
//! [`LayeredFileSource`]: scarab_config_file::LayeredFileSource
//! [`ConfigSource`]: scarab_config::ConfigSource

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::{Path, PathBuf};

use scarab_config::ConfigSource;
use scarab_config_file::{FileSource, LayeredFileSource};
use serde::de::DeserializeOwned;

/// An env lookup a source accepts instead of reading the process
/// environment — `&dyn Fn(&str) -> Option<String>`.
pub type EnvLookup<'a> = dyn Fn(&str) -> Option<String> + 'a;

/// A config source whose `load` can be driven with an injected env
/// lookup. Implemented for the file sources in `scarab-config-file`;
/// downstream env/remote sources implement it the same way so one
/// [`assert_discovery_conformance`] call checks them all.
pub trait EnvAwareSource<T> {
    /// Load using `env` for env-var lookups (e.g. `$<NAME>_CONFIG`).
    ///
    /// # Errors
    /// Same contract as [`ConfigSource::load`].
    fn load_with_env(&self, env: &EnvLookup<'_>) -> Result<T, String>;
}

impl<T> EnvAwareSource<T> for FileSource<T> {
    fn load_with_env(&self, env: &EnvLookup<'_>) -> Result<T, String> {
        self.load_with(env)
    }
}

impl<T> EnvAwareSource<T> for LayeredFileSource<T>
where
    T: DeserializeOwned,
{
    fn load_with_env(&self, env: &EnvLookup<'_>) -> Result<T, String> {
        self.load_with(env)
    }
}

/// How an env var's value is resolved in a [`DiscoveryCase`].
#[derive(Clone, Copy, Debug)]
pub enum EnvValue {
    /// A path relative to the case root — resolved to an absolute
    /// path string (what `$<NAME>_CONFIG` expects).
    Path(&'static str),
    /// A literal string value.
    Literal(&'static str),
}

/// What a [`DiscoveryCase`] expects `load_with_env` to produce.
#[derive(Clone, Debug)]
pub enum Expect<T> {
    /// Load succeeds and equals this model.
    Loaded(T),
    /// Load fails and the error contains this substring.
    Error(&'static str),
}

/// A scripted discovery scenario: files written under a temp root,
/// search dirs + an optional explicit path (both relative to root),
/// env vars (paths resolve against root), and the expected outcome.
///
/// Build with [`DiscoveryCase::loaded`] / [`DiscoveryCase::error`]
/// then chain the `file`/`dir`/`env_*`/`explicit` setters.
pub struct DiscoveryCase<T> {
    name: &'static str,
    files: Vec<(String, String)>,
    dirs: Vec<String>,
    env: Vec<(&'static str, EnvValue)>,
    explicit: Option<String>,
    expect: Expect<T>,
}

impl<T> DiscoveryCase<T> {
    /// A case expecting a successful load equal to `expected`.
    pub fn loaded(name: &'static str, expected: T) -> Self {
        Self {
            name,
            files: Vec::new(),
            dirs: Vec::new(),
            env: Vec::new(),
            explicit: None,
            expect: Expect::Loaded(expected),
        }
    }

    /// A case expecting a load error containing `needle`.
    pub fn error(name: &'static str, needle: &'static str) -> Self {
        Self {
            name,
            files: Vec::new(),
            dirs: Vec::new(),
            env: Vec::new(),
            explicit: None,
            expect: Expect::Error(needle),
        }
    }

    /// Write `text` at `rel` under the case root (parent dirs are
    /// created).
    #[must_use]
    pub fn file(mut self, rel: impl Into<String>, text: impl Into<String>) -> Self {
        self.files.push((rel.into(), text.into()));
        self
    }

    /// Append a search dir (relative to root) — order is search order.
    #[must_use]
    pub fn dir(mut self, rel: impl Into<String>) -> Self {
        self.dirs.push(rel.into());
        self
    }

    /// Set an env var to a case-root-relative path.
    #[must_use]
    pub fn env_path(mut self, var: &'static str, rel: &'static str) -> Self {
        self.env.push((var, EnvValue::Path(rel)));
        self
    }

    /// Set an env var to a literal value.
    #[must_use]
    pub fn env_literal(mut self, var: &'static str, value: &'static str) -> Self {
        self.env.push((var, EnvValue::Literal(value)));
        self
    }

    /// Set the explicit `--config`-style path (relative to root).
    #[must_use]
    pub fn explicit(mut self, rel: impl Into<String>) -> Self {
        self.explicit = Some(rel.into());
        self
    }

    /// The scenario name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.name
    }
}

/// A [`DiscoveryCase`] materialized on disk — temp root, absolute
/// search dirs, resolved explicit path, and the env map. Handed to
/// the `make` factory in [`assert_discovery_conformance`]; the temp
/// dir lives as long as the `PreparedCase`.
pub struct PreparedCase {
    root: PathBuf,
    dirs: Vec<PathBuf>,
    explicit: Option<PathBuf>,
    env: BTreeMap<String, String>,
    _guard: tempfile::TempDir,
}

impl PreparedCase {
    /// The temp root every relative path resolves against.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Absolute search dirs, in declared order.
    #[must_use]
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    /// The resolved explicit path, if the case set one.
    #[must_use]
    pub fn explicit(&self) -> Option<&Path> {
        self.explicit.as_deref()
    }

    /// `root.join(rel)` — absolute path of a case-relative file.
    #[must_use]
    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// An env lookup closure over this case's env map — pass `&env`
    /// to `load_with`/`load_with_env`.
    pub fn env_lookup(&self) -> impl Fn(&str) -> Option<String> + '_ {
        let env = &self.env;
        move |k| env.get(k).cloned()
    }

    /// Materialize `case`: write files, resolve dirs/explicit/env.
    fn prepare<T>(case: &DiscoveryCase<T>) -> Self {
        let guard =
            tempfile::tempdir().unwrap_or_else(|e| panic!("case {}: tempdir: {e}", case.name));
        let root = guard.path().to_path_buf();
        for (rel, text) in &case.files {
            let path = root.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap_or_else(|e| {
                    panic!("case {}: mkdir {}: {e}", case.name, parent.display())
                });
            }
            std::fs::write(&path, text)
                .unwrap_or_else(|e| panic!("case {}: write {}: {e}", case.name, path.display()));
        }
        let env = case
            .env
            .iter()
            .map(|(var, value)| {
                let v = match value {
                    EnvValue::Path(rel) => root.join(rel).to_string_lossy().into_owned(),
                    EnvValue::Literal(s) => (*s).to_string(),
                };
                ((*var).to_string(), v)
            })
            .collect();
        let dirs = case.dirs.iter().map(|d| root.join(d)).collect();
        let explicit = case.explicit.as_ref().map(|e| root.join(e));
        Self {
            root,
            dirs,
            explicit,
            env,
            _guard: guard,
        }
    }
}

/// Run `case` against a source built by `make` and assert the
/// expected outcome. `make` receives the [`PreparedCase`] so it can
/// wire `pc.dirs()`, `pc.explicit()`, and capture any extra config
/// (e.g. layered `overrides`) into the source it returns.
///
/// ```ignore
/// assert_discovery_conformance(
///     &DiscoveryCase::loaded("yaml-wins", cfg!())
///         .file("d/myapp.yaml", "name: yaml\n")
///         .file("d/myapp.toml", "name = \"toml\"\n")
///         .dir("d"),
///     |pc| FileSource::new(Discovery::bare("myapp").dirs(pc.dirs().to_vec())),
/// );
/// ```
///
/// # Panics
/// On setup failure or a violated expectation — this is a test helper.
pub fn assert_discovery_conformance<T, S>(
    case: &DiscoveryCase<T>,
    make: impl FnOnce(&PreparedCase) -> S,
) where
    T: PartialEq + Debug,
    S: EnvAwareSource<T>,
{
    let prepared = PreparedCase::prepare(case);
    let source = make(&prepared);
    let env = prepared.env_lookup();
    let result = source.load_with_env(&env);
    match (&case.expect, result) {
        (Expect::Loaded(expected), Ok(value)) => {
            assert_eq!(&value, expected, "case {}: wrong config", case.name);
        }
        (Expect::Loaded(_), Err(e)) => {
            panic!("case {}: expected load, got Err: {e}", case.name);
        }
        (Expect::Error(needle), Err(e)) => {
            assert!(
                e.contains(needle),
                "case {}: error {e:?} lacks expected substring {needle:?}",
                case.name
            );
        }
        (Expect::Error(_), Ok(_)) => {
            panic!("case {}: expected error, load succeeded", case.name);
        }
    }
}

/// The minimal [`ConfigSource`] contract for sources that need no
/// injected env: `load()` returns `expected`.
///
/// # Panics
/// On an `Err` or a mismatched value.
pub fn assert_config_source_conformance<T, S>(source: &S, expected: &T)
where
    T: PartialEq + Debug,
    S: ConfigSource<T> + ?Sized,
{
    match source.load() {
        Ok(value) => assert_eq!(&value, expected, "config source: wrong config"),
        Err(e) => panic!("config source: expected load, got Err: {e}"),
    }
}

/// Assert `source.load()` fails with an error containing `needle`.
///
/// # Panics
/// On a successful load or an error missing `needle`.
pub fn assert_config_source_error<T, S>(source: &S, needle: &str)
where
    T: PartialEq + Debug,
    S: ConfigSource<T> + ?Sized,
{
    match source.load() {
        Ok(_) => panic!("config source: expected error containing {needle:?}, load succeeded"),
        Err(e) => assert!(
            e.contains(needle),
            "config source: error {e:?} lacks expected substring {needle:?}"
        ),
    }
}
