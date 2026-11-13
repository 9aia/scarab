//! Pack composition — the agnostic `template.extends` contract.
//!
//! This module is the *specification* half of composition: it defines
//! the manifest vocabulary ([`Extends`], [`ParentDecl`],
//! [`MergeRules`]) and the evaluation contract ([`compose_chain`],
//! [`apply_into_policy`]) as data + ports. It deliberately knows
//! nothing about *where* parents come from — resolution is delegated
//! to a [`ParentLocator`] (tamias installs materialize parents under
//! `.scarab-parent*/`; another composition source — local dirs, a
//! handl kit, an OCI-ish registry — implements its own locator
//! against the same spec). See `COMPOSITION.md` for the full spec a
//! second evaluator can implement.
//!
//! ```text
//! template:
//!   extends: git:org/base            # one parent (string form)
//!   # extends:                       # or an ordered list —
//!   #   - git:org/platform           #   lowest precedence first
//!   #   - source: corp/overlay       #   structured form
//!   #     via: tamias-ref           #   resolution-scheme hint
//!   merge:                           # optional — defaults shown
//!     vars: child-wins               #   ancestor|descendant var defaults
//!     into: child-wins               #   into-conflict policy
//!     max-depth: 8                   #   ancestor nesting cap
//! ```

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{PackTemplate, TemplateError};

/// Default ancestor-nesting cap when `merge.max-depth` is absent —
/// same bound tamias applies to `extends` recursion at install time.
pub const DEFAULT_MAX_DEPTH: usize = 8;

/// `template.extends` — the pack's declared parents.
///
/// Accepts three manifest spellings (all normalize to an ordered
/// parent list, **lowest precedence first**):
///
/// - `"extends": "<source>"` — one parent, bare source string.
/// - `"extends": {"source": "<source>", "via": "<scheme>"}` — one
///   structured parent.
/// - `"extends": ["<source>", {"source": …}, …]` — several parents.
///
/// `None`/absent = the pack has no declared parents (materialized
/// parents may still be honored — vendored composition).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Extends {
    /// Several parents, lowest precedence first. Tried FIRST:
    /// untagged matching is ordered, and a seq must not fall through
    /// to [`ParentDecl::Spec`] (serde accepts struct-from-seq).
    Many(Vec<ParentDecl>),
    /// A single parent declaration.
    One(ParentDecl),
}

impl Extends {
    /// The declared parents in precedence order (lowest first).
    #[must_use]
    pub fn parents(&self) -> &[ParentDecl] {
        match self {
            Self::One(p) => std::slice::from_ref(p),
            Self::Many(v) => v,
        }
    }

    /// Source strings, for diagnostics.
    #[must_use]
    pub fn sources(&self) -> Vec<&str> {
        self.parents().iter().map(ParentDecl::source).collect()
    }
}

/// One declared parent — *what* to compose with. The `source` grammar
/// belongs to the resolver selected by `via`; scarab never parses it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParentDecl {
    /// Bare source locator (`"git:org/base"`, `"my-pack"`, a path, …).
    Ref(String),
    /// Structured declaration.
    Spec(ParentSpec),
}

impl ParentDecl {
    /// The opaque source locator.
    #[must_use]
    pub fn source(&self) -> &str {
        match self {
            Self::Ref(s) | Self::Spec(ParentSpec { source: s, .. }) => s,
        }
    }

    /// The optional resolution-scheme hint (`None` = the ambient
    /// resolver's default — for tamias installs, `tamias-ref`).
    #[must_use]
    pub fn via(&self) -> Option<&str> {
        match self {
            Self::Ref(_) => None,
            Self::Spec(p) => p.via.as_deref(),
        }
    }
}

/// Structured [`ParentDecl`] form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParentSpec {
    /// Opaque source locator — grammar is the resolver's business
    /// (tamias ref, store name, local path, kit ref, …).
    pub source: String,
    /// Optional resolution-scheme hint. Absent = the ambient
    /// resolver's default. Resolvers reject schemes they don't
    /// implement — an unknown `via` is a resolution error, never a
    /// silent fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

/// `template.merge` — declared merge semantics as *data*. Every
/// evaluator (the tamias driver today; a handl-lang eval or another
/// engine later) implements the same rules; see `COMPOSITION.md`.
///
/// The rules on the **installing (root) pack** govern the whole
/// composition — ancestor packs' own `merge` tables describe how
/// *they* compose when installed directly, not how they merge into a
/// descendant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct MergeRules {
    /// Variable-default precedence across the chain.
    pub vars: VarsMerge,
    /// `into` conflict policy.
    pub into: IntoMerge,
    /// Ancestor-nesting cap (default [`DEFAULT_MAX_DEPTH`]; `0` is
    /// rejected — use a positive bound).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,
}

/// How `template.vars` defaults layer across an `extends` chain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VarsMerge {
    /// Descendant defaults override ancestor defaults (default —
    /// the child is "closer" to the deployment).
    #[default]
    ChildWins,
    /// Ancestor defaults override descendant defaults.
    ParentWins,
}

/// How duplicate `into` targets across the chain resolve.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IntoMerge {
    /// The later (more-descendant) pack's file wins (default).
    #[default]
    ChildWins,
    /// The earlier (more-ancestral) pack's file wins.
    ParentWins,
    /// Any duplicate `into` is a hard error.
    Error,
}

/// One pack in a composed chain — its parsed `template:` table, its
/// payload dir (the `src` root), and manifest name. Chains are
/// ordered **ancestor-first**: every pack appears after all of its
/// ancestors and before its descendants; the installing pack is last.
#[derive(Debug)]
pub struct ChainEntry {
    /// The pack's `template:` table.
    pub pack: PackTemplate,
    /// Payload dir — resolves `files[].src` against this.
    pub dir: PathBuf,
    /// Manifest `name` (or `?` when absent).
    pub name: String,
}

/// Loads one pack's `template:` table + manifest name from a payload
/// dir — implemented by the driver/adapter that owns manifest
/// parsing (the port never reads `tamias.yaml` itself).
pub trait PackLoader {
    /// Load the pack at `dir`.
    ///
    /// # Errors
    /// [`TemplateError::Compose`] on unreadable/invalid manifests.
    fn load(&self, dir: &Path) -> Result<(PackTemplate, String), TemplateError>;
}

/// Locates the materialized parent payload dirs for a pack — the
/// seam that keeps composition agnostic about *where parents come
/// from*. The returned dirs are payload roots (each carrying its own
/// manifest + materialized ancestors, recursively).
///
/// Implementations:
///
/// - `tamias-driver-template`'s materialized locator — reads the
///   `.scarab-parent/` (single) / `.scarab-parents/<i>/` (multi)
///   convention tamias installs write.
/// - A hypothetical handl/path resolver — returns dirs it materialized
///   itself; the rest of the contract is identical.
pub trait ParentLocator {
    /// Parent payload dirs for `payload`, in declaration/precedence
    /// order (lowest first). `extends` is the pack's declaration —
    /// `None` when it declares no parents (a locator may still return
    /// *vendored* parents it finds on disk).
    ///
    /// # Errors
    /// [`TemplateError::Compose`] on unreadable/malformed layouts.
    fn locate(
        &self,
        payload: &Path,
        extends: Option<&Extends>,
    ) -> Result<Vec<PathBuf>, TemplateError>;
}

/// Walk the `extends` chain from `root` and return every pack
/// **ancestor-first** — the render/precedence order: ancestors render
/// before descendants, so on conflicts the descendant wins (subject to
/// [`MergeRules::into`]).
///
/// Semantics (the portable contract — see `COMPOSITION.md`):
///
/// - A pack's parents render in declaration order, lowest precedence
///   first; each parent's own ancestors render before it (post-order).
/// - A shared ancestor reachable through several paths renders **once**,
///   at its first occurrence — diamonds collapse to tree order.
/// - Declared `extends` with no/fewer/more materialized parents than
///   declared is an error ([`TemplateError::MissingParents`]);
///   materialized parents without a declaration are honored (vendored
///   composition).
/// - The root pack's `merge.max-depth` caps nesting
///   ([`DEFAULT_MAX_DEPTH`] when absent); cycles through materialized
///   dirs terminate (each dir is visited once).
///
/// # Errors
/// [`TemplateError::Compose`]/[`TemplateError::MissingParents`] on
/// malformed declarations, locator/loader failures, depth-cap or
/// cycle-pathology breaches.
pub fn compose_chain<L, R>(
    loader: &L,
    locator: &R,
    root: &Path,
) -> Result<Vec<ChainEntry>, TemplateError>
where
    L: PackLoader,
    R: ParentLocator,
{
    let (root_pack, _) = loader.load(root)?;
    let limit = match root_pack.merge.max_depth {
        Some(0) => {
            return Err(TemplateError::Compose(
                "merge.max-depth must be >= 1".to_string(),
            ));
        }
        Some(n) => n,
        None => DEFAULT_MAX_DEPTH,
    };
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    walk(loader, locator, root, 0, limit, &mut seen, &mut out)?;
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn walk<L, R>(
    loader: &L,
    locator: &R,
    dir: &Path,
    depth: usize,
    limit: usize,
    seen: &mut BTreeSet<PathBuf>,
    out: &mut Vec<ChainEntry>,
) -> Result<(), TemplateError>
where
    L: PackLoader,
    R: ParentLocator,
{
    let key = normalize(dir);
    if !seen.insert(key) {
        // Already emitted (diamond) or an in-progress ancestor
        // (materialized cycle) — either way, don't descend again.
        return Ok(());
    }
    if depth > limit {
        return Err(TemplateError::Compose(format!(
            "extends chain under {} exceeds the {limit}-deep cap (merge.max-depth)",
            dir.display()
        )));
    }
    let (pack, name) = loader.load(dir)?;
    let extends = pack.extends.clone();
    let located = locator.locate(dir, extends.as_ref())?;
    let parents: &[PathBuf] = match &extends {
        Some(e) => {
            let decls = e.parents();
            if decls.is_empty() {
                return Err(TemplateError::Compose(
                    "template.extends is an empty list — name at least one parent".to_string(),
                ));
            }
            for d in decls {
                if d.source().trim().is_empty() {
                    return Err(TemplateError::Compose(
                        "template.extends parent has an empty source".to_string(),
                    ));
                }
            }
            if located.len() != decls.len() {
                return Err(TemplateError::MissingParents {
                    declared: decls.len(),
                    found: located.len(),
                    dir: dir.display().to_string(),
                });
            }
            &located[..]
        }
        // No declaration — whatever the locator found is vendored
        // composition and is honored as-is.
        None => &located[..],
    };
    for parent in parents {
        walk(loader, locator, parent, depth + 1, limit, seen, out)?;
    }
    out.push(ChainEntry {
        pack,
        dir: dir.to_path_buf(),
        name,
    });
    Ok(())
}

/// Apply the declared [`IntoMerge`] policy to an ordered render list
/// (ancestor-first, duplicates adjacent-or-not). Returns the files to
/// actually write, in render order:
///
/// - [`IntoMerge::ChildWins`] — keep each `into`'s *last* occurrence
///   (the descendant's render).
/// - [`IntoMerge::ParentWins`] — keep each `into`'s *first* occurrence
///   (the ancestor's render).
/// - [`IntoMerge::Error`] — any duplicate `into` fails with
///   [`TemplateError::Compose`].
///
/// # Errors
/// [`TemplateError::Compose`] on `into` conflicts under
/// [`IntoMerge::Error`].
pub fn apply_into_policy(
    rendered: Vec<(PathBuf, String)>,
    policy: IntoMerge,
) -> Result<Vec<(PathBuf, String)>, TemplateError> {
    match policy {
        IntoMerge::ChildWins => {
            let mut seen = BTreeSet::new();
            let mut kept: Vec<(PathBuf, String)> = Vec::with_capacity(rendered.len());
            for item in rendered.into_iter().rev() {
                if seen.insert(item.0.clone()) {
                    kept.push(item);
                }
            }
            kept.reverse();
            Ok(kept)
        }
        IntoMerge::ParentWins => {
            let mut seen = BTreeSet::new();
            Ok(rendered
                .into_iter()
                .filter(|(into, _)| seen.insert(into.clone()))
                .collect())
        }
        IntoMerge::Error => {
            let mut seen = BTreeSet::new();
            for (into, _) in &rendered {
                if !seen.insert(into.clone()) {
                    return Err(TemplateError::Compose(format!(
                        "into conflict on {} (merge.into = error)",
                        into.display()
                    )));
                }
            }
            Ok(rendered)
        }
    }
}

/// Lexically normalize a path for dedup keys — resolves `.`/`..`
/// without touching the filesystem (keeps this module I/O-free).
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use serde_json::json;

    /// Parse a `PackTemplate` from a JSON `template:` table.
    fn pack(json: serde_json::Value) -> PackTemplate {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn extends_string_and_table_and_list_parse() {
        let p = pack(json!({"extends": "git:org/base"}));
        assert_eq!(
            p.extends.as_ref().map(Extends::sources),
            Some(vec!["git:org/base"])
        );

        let p = pack(json!({"extends": {"source": "git:org/base", "via": "tamias-ref"}}));
        let e = p.extends.unwrap();
        assert_eq!(e.sources(), ["git:org/base"]);
        assert_eq!(e.parents()[0].via(), Some("tamias-ref"));

        let p = pack(json!({"extends": ["git:a", {"source": "git:b"}]}));
        assert_eq!(p.extends.unwrap().sources(), ["git:a", "git:b"]);

        let p = pack(json!({"files": []}));
        assert!(p.extends.is_none());
        // merge defaults
        assert_eq!(p.merge.vars, VarsMerge::ChildWins);
        assert_eq!(p.merge.into, IntoMerge::ChildWins);
        assert_eq!(p.merge.max_depth, None);
    }

    #[test]
    fn merge_rules_parse_kebab_case() {
        let p = pack(json!({
            "merge": {"vars": "parent-wins", "into": "error", "max-depth": 4}
        }));
        assert_eq!(p.merge.vars, VarsMerge::ParentWins);
        assert_eq!(p.merge.into, IntoMerge::Error);
        assert_eq!(p.merge.max_depth, Some(4));
    }

    /// In-memory pack tree: dir → (extends sources as located dirs,
    /// template json). `locate` maps each pack's declared extends to
    /// child dirs by name suffix.
    struct Mem {
        /// dir path → (PackTemplate json value, name)
        packs: std::collections::BTreeMap<PathBuf, (serde_json::Value, String)>,
        /// dir → parent dirs (declaration order)
        parents: std::collections::BTreeMap<PathBuf, Vec<PathBuf>>,
    }

    impl PackLoader for Mem {
        fn load(&self, dir: &Path) -> Result<(PackTemplate, String), TemplateError> {
            let (v, name) = self
                .packs
                .get(&normalize(dir))
                .ok_or_else(|| TemplateError::Compose(format!("no pack at {}", dir.display())))?;
            let pack: PackTemplate = serde_json::from_value(v.clone())
                .map_err(|e| TemplateError::Manifest(e.to_string()))?;
            Ok((pack, name.clone()))
        }
    }

    impl ParentLocator for Mem {
        fn locate(
            &self,
            payload: &Path,
            _extends: Option<&Extends>,
        ) -> Result<Vec<PathBuf>, TemplateError> {
            Ok(self
                .parents
                .get(&normalize(payload))
                .cloned()
                .unwrap_or_default())
        }
    }

    fn mem_pack(mem: &mut Mem, dir: &str, name: &str, extends: serde_json::Value) {
        let dir = PathBuf::from(dir);
        let mut v = json!({"files": []});
        if !extends.is_null() {
            v["extends"] = extends;
        }
        mem.packs.insert(normalize(&dir), (v, name.to_string()));
    }

    #[test]
    fn compose_chain_orders_ancestors_first_and_dedups_diamonds() {
        let mut mem = Mem {
            packs: BTreeMap::new(),
            parents: BTreeMap::new(),
        };
        // diamond: P extends [A, B]; A and B each extend C.
        mem_pack(&mut mem, "/p", "p", json!(["/a", "/b"]));
        mem_pack(&mut mem, "/a", "a", json!("/c"));
        mem_pack(&mut mem, "/b", "b", json!("/c"));
        mem_pack(&mut mem, "/c", "c", serde_json::Value::Null);
        mem.parents.insert(
            PathBuf::from("/p"),
            vec![PathBuf::from("/a"), PathBuf::from("/b")],
        );
        mem.parents
            .insert(PathBuf::from("/a"), vec![PathBuf::from("/c")]);
        mem.parents
            .insert(PathBuf::from("/b"), vec![PathBuf::from("/c")]);
        let chain = compose_chain(&mem, &mem, Path::new("/p")).unwrap();
        let names: Vec<&str> = chain.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["c", "a", "b", "p"]);
    }

    #[test]
    fn compose_chain_errors_on_parent_count_mismatch() {
        let mut mem = Mem {
            packs: BTreeMap::new(),
            parents: BTreeMap::new(),
        };
        mem_pack(&mut mem, "/p", "p", json!(["/a", "/b"]));
        mem_pack(&mut mem, "/a", "a", serde_json::Value::Null);
        // only one parent materialized for two declared
        mem.parents
            .insert(PathBuf::from("/p"), vec![PathBuf::from("/a")]);
        let err = compose_chain(&mem, &mem, Path::new("/p")).unwrap_err();
        assert!(
            matches!(err, TemplateError::MissingParents { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn compose_chain_honors_vendored_parents_without_extends() {
        let mut mem = Mem {
            packs: BTreeMap::new(),
            parents: BTreeMap::new(),
        };
        mem_pack(&mut mem, "/p", "p", serde_json::Value::Null);
        mem_pack(&mut mem, "/v", "v", serde_json::Value::Null);
        mem.parents
            .insert(PathBuf::from("/p"), vec![PathBuf::from("/v")]);
        let chain = compose_chain(&mem, &mem, Path::new("/p")).unwrap();
        let names: Vec<&str> = chain.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["v", "p"]);
    }

    #[test]
    fn apply_into_policy_child_and_parent_wins() {
        let rendered = vec![
            (PathBuf::from("x"), "first".to_string()),
            (PathBuf::from("y"), "keep".to_string()),
            (PathBuf::from("x"), "last".to_string()),
        ];
        let out = apply_into_policy(rendered.clone(), IntoMerge::ChildWins).unwrap();
        assert_eq!(
            out,
            vec![
                (PathBuf::from("y"), "keep".into()),
                (PathBuf::from("x"), "last".into())
            ]
        );
        let out = apply_into_policy(rendered.clone(), IntoMerge::ParentWins).unwrap();
        assert_eq!(
            out,
            vec![
                (PathBuf::from("x"), "first".into()),
                (PathBuf::from("y"), "keep".into())
            ]
        );
        assert!(apply_into_policy(rendered, IntoMerge::Error).is_err());
        // no conflicts → all policies pass through
        let clean = vec![(PathBuf::from("a"), "1".to_string())];
        assert_eq!(
            apply_into_policy(clean.clone(), IntoMerge::Error).unwrap(),
            clean
        );
    }

    #[test]
    fn compose_chain_rejects_zero_max_depth() {
        let mut mem = Mem {
            packs: BTreeMap::new(),
            parents: BTreeMap::new(),
        };
        mem.packs.insert(
            PathBuf::from("/p"),
            (json!({"merge": {"max-depth": 0}}), "p".to_string()),
        );
        assert!(compose_chain(&mem, &mem, Path::new("/p")).is_err());
    }
}
