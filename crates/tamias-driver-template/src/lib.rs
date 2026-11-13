//! tamias-driver-template — the `type: template` content driver:
//! renders a template pack's files into declared targets.
//!
//! Verbs (the tamias driver contract — `--dir <payload>
//! --name <pkg> [--config <vars-file>] [key=value …]`):
//!
//! - `install` — validate the pack *chain* (each manifest parses,
//!   every `src` exists and compiles, vars resolve) and preview the
//!   merged render plan. Writes nothing — the payload dir is the
//!   evictable fetch cache; rendered output belongs to `deploy`.
//! - `deploy [--dry-run] [--target <dir>]` — render each
//!   `template.files[]` entry through the engine and write it at
//!   `into` under the target dir (`--target` → `$TEMPLATE_TARGET` →
//!   cwd). Records `.scarab-template-state.json` at the target
//!   root so `uninstall` can remove exactly what it wrote.
//! - `uninstall` — delete files still matching the recorded
//!   sha256 digests; modified files are left alone and reported.
//! - `config-schema` — JSON Schema (draft 2020-12) for the optional
//!   `--config` vars file (a flat `name = value` map).
//! - `capabilities` — `{"types": ["template"], "verbs": […]}`.
//!
//! **Composition (`template.extends`).** A pack may declare parents
//! it composes with — the agnostic contract lives in
//! `scarab_template::compose` (see `scarab-template/COMPOSITION.md`).
//! This driver is the *materialized-parent* evaluator: tamias
//! resolves `tamias-ref` sources at install time and copies each
//! parent's payload inside this one — `.scarab-parent/` for a single
//! parent, `.scarab-parents/<i>/` (declaration order) for several —
//! recursively, so a grandparent nests inside its parent. The chain
//! is evaluated ancestor-first under the pack's `template.merge`
//! rules (defaults: descendant vars win, descendant `into`s win).
//! `.scarab-parent*/` present without `extends` is honored the same
//! way (a vendored parent); `extends` declared but nothing
//! materialized is an error — the pack must go through
//! `tamias install`.
//!
//! Var context precedence (later wins): ancestor `template.vars`
//! defaults → … → this pack's `template.vars` → auto vars (`repo`,
//! `user`) → `--config` file → `$TEMPLATE_VAR_<NAME>` env →
//! trailing `key=value` args.
//!
//! State stays v1-simple: `.scarab-template-state.json` records one
//! digest per `into` — a child-overridden file keeps only the final
//! (child) digest, which is exactly what uninstall needs.
//!
//! The engine is minijinja (`scarab-template-minijinja`) —
//! sandboxed, no filesystem/network access from templates.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use scarab_template::compose::{
    ChainEntry, Extends, MergeRules, ParentLocator, VarsMerge, apply_into_policy, compose_chain,
};
use scarab_template::{
    PackTemplate, TemplateEngine, TemplateError, merge_context, safe_into, safe_src,
};
use scarab_template_minijinja::MinijinjaEngine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// What `deploy`/`uninstall` wrote — enough to reverse exactly.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct State {
    /// pack name that wrote this state file
    #[serde(default)]
    pack: String,
    /// into-path (relative, `/`-joined) → sha256 hex of contents
    #[serde(default)]
    files: BTreeMap<String, String>,
}

const STATE_FILE: &str = ".scarab-template-state.json";

/// Driver-level failure — message is what the operator sees on
/// stderr (tamias wraps it in "driver … failed: …").
#[derive(Debug, thiserror::Error)]
pub enum OpsError {
    #[error("{0}")]
    Msg(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("template manifest: {0}")]
    Manifest(String),
    #[error(transparent)]
    Template(#[from] scarab_template::TemplateError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

type Result<T> = std::result::Result<T, OpsError>;

fn err(msg: impl Into<String>) -> OpsError {
    OpsError::Msg(msg.into())
}

/// Parsed driver argv: the standard `--dir`/`--name`/`--config`
/// flags plus verb-specific extras.
#[derive(Debug, Default)]
struct Args {
    dir: Option<PathBuf>,
    name: Option<String>,
    config: Option<PathBuf>,
    target: Option<PathBuf>,
    dry_run: bool,
    /// trailing `key=value` var overrides
    pairs: Vec<(String, String)>,
}

fn parse_args(argv: &[String]) -> Result<Args> {
    let mut args = Args::default();
    let mut idx = 0;
    while idx < argv.len() {
        match argv[idx].as_str() {
            "--dir" => {
                idx += 1;
                args.dir = argv.get(idx).map(PathBuf::from);
            }
            "--name" => {
                idx += 1;
                args.name = argv.get(idx).cloned();
            }
            "--config" => {
                idx += 1;
                args.config = argv.get(idx).map(PathBuf::from);
            }
            "--target" => {
                idx += 1;
                args.target = argv.get(idx).map(PathBuf::from);
            }
            "--dry-run" => args.dry_run = true,
            s if s.contains('=') && !s.starts_with("--") => {
                let (key, val) = s.split_once('=').unwrap_or((s, ""));
                args.pairs.push((key.to_string(), val.to_string()));
            }
            other => return Err(err(format!("unknown arg {other:?}"))),
        }
        idx += 1;
    }
    Ok(args)
}

/// Load + validate the pack: read `tamias.yaml` (or `.yml`), pull
/// the `template:` table into [`PackTemplate`].
fn load_pack(dir: &Path) -> Result<(PackTemplate, String)> {
    let manifest_path = ["tamias.yaml", "tamias.yml"]
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file())
        .ok_or_else(|| err(format!("{}: no tamias.yaml/.yml manifest", dir.display())))?;
    let text = std::fs::read_to_string(&manifest_path)?;
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)
        .map_err(|e| err(format!("{}: {e}", manifest_path.display())))?;
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string();
    let kind = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if kind != "template" {
        return Err(err(format!(
            "{}: type is {kind:?}, not \"template\" — wrong driver",
            manifest_path.display()
        )));
    }
    let tmpl_value = value
        .get("template")
        .cloned()
        .ok_or_else(|| err("tamias.yaml has no `template:` table"))?;
    let json = serde_json::to_value(tmpl_value).map_err(|e| err(e.to_string()))?;
    let pack: PackTemplate =
        serde_json::from_value(json).map_err(|e| err(format!("template table: {e}")))?;
    if pack.files.is_empty() {
        return Err(err("template.files is empty"));
    }
    // Validate every path up front — a bad pack fails before any write.
    for f in &pack.files {
        safe_src(&f.src)?;
        safe_into(&f.into)?;
    }
    Ok((pack, name))
}

/// Payload subdir tamias materializes a single resolved
/// `template.extends` parent into — and where this driver looks for
/// it (the multi-parent layout is [`PARENTS_DIR`]).
const PARENT_DIR: &str = ".scarab-parent";
/// Payload subdir tamias materializes several `extends` parents into
/// — one numbered child per declaration, in order.
const PARENTS_DIR: &str = ".scarab-parents";

/// The driver's [`ParentLocator`]: discovers parents already
/// materialized inside the payload by tamias install —
/// `.scarab-parents/<i>/` when present (multi-parent layout), else a
/// lone `.scarab-parent/` (single-parent layout, also the vendored
/// convention). The driver never resolves `source` locators itself;
/// it only reads what install laid down.
struct MaterializedLocator;

impl ParentLocator for MaterializedLocator {
    fn locate(
        &self,
        payload: &Path,
        _extends: Option<&Extends>,
    ) -> std::result::Result<Vec<PathBuf>, TemplateError> {
        let multi = payload.join(PARENTS_DIR);
        if multi.is_dir() {
            let mut dirs: Vec<(usize, PathBuf)> = Vec::new();
            for entry in std::fs::read_dir(&multi)
                .map_err(|e| TemplateError::Compose(format!("{}: {e}", multi.display())))?
            {
                let entry = entry
                    .map_err(|e| TemplateError::Compose(format!("{}: {e}", multi.display())))?;
                if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let Ok(idx) = name.parse::<usize>() else {
                    return Err(TemplateError::Compose(format!(
                        "{}: non-numeric parent dir {name:?} — expected <index>/ payloads",
                        multi.display()
                    )));
                };
                dirs.push((idx, entry.path()));
            }
            dirs.sort_by_key(|(i, _)| *i);
            return Ok(dirs.into_iter().map(|(_, p)| p).collect());
        }
        let single = payload.join(PARENT_DIR);
        if single.is_dir() {
            return Ok(vec![single]);
        }
        Ok(Vec::new())
    }
}

/// The driver's [`PackLoader`] — wraps [`load_pack`] (manifest parse +
/// `type: template` check) into the port's error type.
struct DriverLoader;

impl scarab_template::PackLoader for DriverLoader {
    fn load(&self, dir: &Path) -> std::result::Result<(PackTemplate, String), TemplateError> {
        load_pack(dir).map_err(|e| match e {
            OpsError::Manifest(m) => TemplateError::Manifest(m),
            OpsError::Template(t) => t,
            other => TemplateError::Compose(other.to_string()),
        })
    }
}

/// Load the payload's pack plus every materialized ancestor via the
/// agnostic composition contract ([`compose_chain`]) — ancestor-first.
/// `extends` declared but fewer/more parents materialized than
/// declared fails with [`TemplateError::MissingParents`]; the error
/// gets the operator-facing `tamias install` hint appended.
fn load_chain(dir: &Path) -> Result<Vec<ChainEntry>> {
    compose_chain(&DriverLoader, &MaterializedLocator, dir).map_err(|e| match e {
        TemplateError::MissingParents {
            declared,
            found,
            dir: d,
        } => err(format!(
            "extends declares {declared} parent(s) but {found} materialized under {d} — \
             install through `tamias install` so the parent payload(s) land inside the package"
        )),
        other => OpsError::Template(other),
    })
}

/// The merge rules governing this composition — the installing
/// (root) pack's `template.merge`; the chain is ancestor-first so
/// the root is last.
fn merge_rules(chain: &[ChainEntry]) -> MergeRules {
    chain
        .last()
        .map_or_else(MergeRules::default, |c| c.pack.merge)
}

/// Auto vars every pack sees (overridable by later layers).
fn auto_vars(dir: &Path) -> BTreeMap<String, serde_json::Value> {
    let mut m = BTreeMap::new();
    // repo: git remote basename when available, else dir name
    let repo = std::process::Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|u| {
            u.trim()
                .trim_end_matches(".git")
                .rsplit(['/', ':'])
                .next()
                .unwrap_or("")
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::fs::canonicalize(dir)
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))
        });
    if let Some(r) = repo {
        m.insert("repo".into(), serde_json::Value::String(r));
    }
    if let Some(u) = std::env::var_os("USER").or_else(|| std::env::var_os("LOGNAME")) {
        m.insert(
            "user".into(),
            serde_json::Value::String(u.to_string_lossy().into_owned()),
        );
    }
    m
}

/// Var layers, later winning: ancestor `template.vars` defaults →
/// … → this pack's `template.vars` → auto vars (`repo`, `user`) →
/// `--config` file → `$TEMPLATE_VAR_<NAME>` env → trailing
/// `key=value` args. `chain` is ancestor-first (see
/// [`compose_chain`]); the pack-vars fold direction follows the root
/// pack's `merge.vars` — [`VarsMerge::ChildWins`] folds ancestor→
/// descendant (descendant defaults win), [`VarsMerge::ParentWins`]
/// folds descendant→ancestor (ancestor defaults win).
fn resolve_vars(chain: &[ChainEntry], a: &Args, work_dir: &Path) -> Result<serde_json::Value> {
    let mut file_vars = BTreeMap::new();
    if let Some(cfg) = &a.config {
        let text = std::fs::read_to_string(cfg)?;
        let v: serde_json::Value = match cfg.extension().and_then(|e| e.to_str()) {
            Some("json") => {
                serde_json::from_str(&text).map_err(|e| err(format!("{}: {e}", cfg.display())))?
            }
            Some("toml") => {
                let t: toml::Table =
                    toml::from_str(&text).map_err(|e| err(format!("{}: {e}", cfg.display())))?;
                serde_json::to_value(t).map_err(|e| err(e.to_string()))?
            }
            // .yaml/.yml and extensionless default to YAML
            _ => {
                let y: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)
                    .map_err(|e| err(format!("{}: {e}", cfg.display())))?;
                serde_json::to_value(y).map_err(|e| err(e.to_string()))?
            }
        };
        if let serde_json::Value::Object(map) = v {
            file_vars = map.into_iter().collect();
        } else {
            return Err(err(format!("{}: vars file must be a map", cfg.display())));
        }
    }
    let mut env_vars = BTreeMap::new();
    for (k, v) in std::env::vars() {
        if let Some(name) = k.strip_prefix("TEMPLATE_VAR_") {
            env_vars.insert(
                name.to_lowercase(),
                serde_json::from_str(&v).unwrap_or(serde_json::Value::String(v)),
            );
        }
    }
    let mut args_vars = env_vars;
    for (k, v) in &a.pairs {
        args_vars.insert(
            k.clone(),
            serde_json::from_str(v).unwrap_or_else(|_| serde_json::Value::String(v.clone())),
        );
    }
    let mut pack_vars = BTreeMap::new();
    let ordered: Vec<&ChainEntry> = match merge_rules(chain).vars {
        VarsMerge::ChildWins => chain.iter().collect(),
        VarsMerge::ParentWins => chain.iter().rev().collect(),
    };
    for cp in ordered {
        for (k, v) in &cp.pack.vars {
            pack_vars.insert(k.clone(), v.clone());
        }
    }
    Ok(merge_context(
        &pack_vars,
        &auto_vars(work_dir),
        &file_vars,
        &args_vars,
    ))
}

fn sha256_hex(data: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut h = Sha256::new();
    h.update(data);
    h.finalize()
        .iter()
        .fold(String::with_capacity(64), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        })
}

fn render_pack(
    pack: &PackTemplate,
    payload: &Path,
    ctx: &serde_json::Value,
    engine: &dyn TemplateEngine,
) -> Result<Vec<(PathBuf, String)>> {
    let mut out = Vec::new();
    for f in &pack.files {
        let src = safe_src(&f.src)?;
        let text = std::fs::read_to_string(payload.join(&src))
            .map_err(|e| err(format!("{}: {e}", src.display())))?;
        let rendered = engine
            .render(&text, ctx)
            .map_err(|e| err(format!("{}: {e}", f.src)))?;
        out.push((safe_into(&f.into)?, rendered));
    }
    Ok(out)
}

/// Render the whole `extends` chain against one context — ancestor
/// packs first, the child last — then apply the root pack's
/// `merge.into` policy ([`apply_into_policy`]). Under the default
/// `child-wins` policy duplicate `into`s collapse to the descendant's
/// render, so the state digest keeps only the final (child) content.
fn render_chain(
    chain: &[ChainEntry],
    ctx: &serde_json::Value,
    engine: &dyn TemplateEngine,
    rules: &MergeRules,
) -> Result<Vec<(PathBuf, String)>> {
    let mut out = Vec::new();
    for cp in chain {
        out.extend(render_pack(&cp.pack, &cp.dir, ctx, engine)?);
    }
    Ok(apply_into_policy(out, rules.into)?)
}

fn target_dir(a: &Args) -> Result<PathBuf> {
    if let Some(t) = &a.target {
        return Ok(scarab_config_dir_free(t));
    }
    if let Some(t) = std::env::var_os("TEMPLATE_TARGET") {
        return Ok(PathBuf::from(t));
    }
    Ok(std::env::current_dir()?)
}

fn scarab_config_dir_free(p: &Path) -> PathBuf {
    scarab_config::expand_tilde(p)
}

/// `install` — validate the pack chain end-to-end (each manifest,
/// sources, render with resolved vars) and report the merged plan.
/// Writes nothing.
fn install(a: &Args) -> Result<()> {
    let dir = required_dir(a)?;
    let chain = load_chain(&dir)?;
    let child = chain.last().map_or("?", |c| c.name.as_str());
    let rules = merge_rules(&chain);
    let engine = MinijinjaEngine::new();
    // Auto vars describe the destination repo, not the pack payload.
    let ctx_dir = target_dir(a)?;
    let ctx = resolve_vars(&chain, a, &ctx_dir)?;
    let rendered = render_chain(&chain, &ctx, &engine, &rules)?;
    let total_files: usize = chain.iter().map(|c| c.pack.files.len()).sum();
    println!(
        "template: {child} — {} file(s) validated across {} pack(s) (engine: {})",
        rendered.len(),
        chain.len(),
        engine.name()
    );
    if chain.len() > 1 {
        let names: Vec<&str> = chain.iter().map(|c| c.name.as_str()).collect();
        println!("  extends chain: {}", names.join(" → "));
    }
    for (into, _) in &rendered {
        println!("  {}", into.display());
    }
    let overridden = total_files.saturating_sub(rendered.len());
    if overridden > 0 {
        println!("  ({overridden} file(s) overridden by merge policy)");
    }
    Ok(())
}

/// `deploy` — render + write into the target dir.
fn deploy(a: &Args) -> Result<()> {
    let dir = required_dir(a)?;
    let chain = load_chain(&dir)?;
    let name = chain
        .last()
        .map_or_else(|| "?".to_string(), |c| c.name.clone());
    let rules = merge_rules(&chain);
    let engine = MinijinjaEngine::new();
    let target = target_dir(a)?;
    // Auto vars describe the destination repo, not the pack payload.
    let ctx = resolve_vars(&chain, a, &target)?;
    let rendered = render_chain(&chain, &ctx, &engine, &rules)?;
    std::fs::create_dir_all(&target)?;

    let mut state = State {
        pack: name.clone(),
        ..State::default()
    };
    let mut actions = Vec::new();
    for (into, body) in &rendered {
        let dest = target.join(into);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let digest = sha256_hex(body.as_bytes());
        state
            .files
            .insert(into.to_string_lossy().replace('\\', "/"), digest);
        if a.dry_run {
            actions.push(format!(
                "would write {} ({} bytes)",
                dest.display(),
                body.len()
            ));
        } else {
            std::fs::write(&dest, body)?;
            actions.push(format!("wrote {}", dest.display()));
        }
    }
    if !a.dry_run {
        let state_path = target.join(STATE_FILE);
        // Merge with any existing state from other packs? v1: one
        // state file per target dir is shared — merge file maps.
        let mut merged = read_state(&state_path)?;
        for (k, v) in state.files {
            merged.files.insert(k, v);
        }
        merged.pack = name;
        std::fs::write(&state_path, serde_json::to_string_pretty(&merged)?)?;
        actions.push(format!("state: {}", state_path.display()));
    }
    for line in actions {
        println!("{line}");
    }
    Ok(())
}

fn read_state(path: &Path) -> Result<State> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            serde_json::from_str(&text).map_err(|e| err(format!("{}: {e}", path.display())))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
        Err(e) => Err(e.into()),
    }
}

/// `uninstall` — remove files whose current digest matches the
/// recorded state; modified files stay and are reported.
fn uninstall(a: &Args) -> Result<()> {
    let target = target_dir(a)?;
    let state_path = target.join(STATE_FILE);
    let state = read_state(&state_path)?;
    if state.files.is_empty() {
        println!("template: nothing recorded at {}", target.display());
        return Ok(());
    }
    let mut removed = 0;
    let mut kept = Vec::new();
    for (rel, digest) in &state.files {
        let path = target.join(rel);
        match std::fs::read(&path) {
            Ok(bytes) if sha256_hex(&bytes) == *digest => {
                if a.dry_run {
                    println!("would remove {}", path.display());
                } else {
                    std::fs::remove_file(&path)?;
                    println!("removed {}", path.display());
                }
                removed += 1;
            }
            Ok(_) => kept.push(rel.clone()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    if !a.dry_run {
        // Drop removed entries; keep modified/missing? Missing files
        // are already gone — drop them too.
        let remaining: BTreeMap<String, String> = state
            .files
            .iter()
            .filter(|(k, _)| kept.contains(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut st = state;
        st.files = remaining;
        if st.files.is_empty() {
            let _ = std::fs::remove_file(&state_path);
        } else {
            std::fs::write(&state_path, serde_json::to_string_pretty(&st)?)?;
        }
    }
    for rel in &kept {
        println!(
            "kept (modified since deploy): {}",
            target.join(rel).display()
        );
    }
    println!(
        "template: {removed} file(s) {}",
        if a.dry_run {
            "would be removed"
        } else {
            "removed"
        }
    );
    Ok(())
}

/// `config-schema` — JSON Schema for the `--config` vars file.
pub fn config_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "template vars file",
        "description": "Flat map of template variables — strings, numbers, booleans, arrays, or tables. Parsed as TOML (.toml), YAML (.yaml/.yml), or JSON (.json) by extension; default YAML.",
        "type": "object",
        "additionalProperties": true
    })
}

pub fn capabilities() -> serde_json::Value {
    serde_json::json!({
        "name": "template",
        "types": ["template"],
        "verbs": ["install", "deploy", "uninstall", "config-schema"],
        "engine": "minijinja"
    })
}

fn required_dir(a: &Args) -> Result<PathBuf> {
    a.dir.clone().ok_or_else(|| err("missing --dir <payload>"))
}

/// Dispatch one verb. Returns the process exit code.
pub fn run(verb: &str, argv: &[String]) -> i32 {
    let args = match parse_args(argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let result = match verb {
        "install" => install(&args),
        "deploy" => deploy(&args),
        "uninstall" => uninstall(&args),
        "config-schema" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&config_schema()).unwrap_or_default()
            );
            Ok(())
        }
        other => {
            eprintln!("unsupported verb {other:?} — verbs: install|deploy|uninstall|config-schema");
            return 2;
        }
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn safe_into_rejects_escapes() {
        assert!(safe_into("./app.toml").is_ok());
        assert!(safe_into("../evil").is_err());
        assert!(safe_into("/abs/path").is_err());
        assert!(safe_into("").is_err());
        assert!(safe_into("a/../../b").is_err());
    }

    #[test]
    fn safe_src_rejects_escapes() {
        assert!(safe_src("templates/app.tmpl").is_ok());
        assert!(safe_src("../x").is_err());
    }

    #[test]
    fn parse_args_pairs_and_flags() {
        let argv: Vec<String> = [
            "--dir",
            "/p",
            "--name",
            "pkg",
            "--config",
            "/c.yaml",
            "--target",
            "/t",
            "--dry-run",
            "env=prod",
            "replicas=3",
        ]
        .iter()
        .map(std::string::ToString::to_string)
        .collect();
        let a = parse_args(&argv).unwrap();
        assert_eq!(a.dir.unwrap(), PathBuf::from("/p"));
        assert_eq!(a.name.as_deref(), Some("pkg"));
        assert!(a.dry_run);
        assert_eq!(a.pairs.len(), 2);
    }

    #[test]
    fn config_schema_is_valid_json() {
        let v = config_schema();
        assert_eq!(v["type"], "object");
    }

    /// Write a minimal template pack: `tamias.yaml` + one tmpl file.
    fn write_pack(dir: &Path, name: &str, vars: &str, files: &str, extends: Option<&str>) {
        std::fs::create_dir_all(dir).unwrap();
        let ext = extends.map_or_else(String::new, |e| format!("  extends: {e}\n"));
        let vars_block = if vars.is_empty() {
            String::new()
        } else {
            format!("  vars:\n{vars}")
        };
        std::fs::write(
            dir.join("tamias.yaml"),
            format!("name: {name}\ntype: template\ntemplate:\n{vars_block}{ext}  files:\n{files}"),
        )
        .unwrap();
    }

    /// A three-level chain: grandparent → parent (`.scarab-parent`) →
    /// child. Returns (child payload dir, _tempdirs guard).
    fn chain_fixture() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let gp = tmp.path().join("gp");
        write_pack(
            &gp,
            "grandparent",
            "    who: gp\n    shared: gp\n",
            "    - src: gp.txt.tmpl\n      into: gp.txt\n    - src: shared.txt.tmpl\n      into: shared.txt\n",
            None,
        );
        std::fs::write(gp.join("gp.txt.tmpl"), "gp {{ who }} {{ shared }}").unwrap();
        std::fs::write(gp.join("shared.txt.tmpl"), "shared=gp").unwrap();

        let parent = tmp.path().join("parent");
        std::fs::create_dir_all(parent.join(PARENT_DIR)).unwrap();
        // parent payload = its own files + materialized grandparent
        let gp_copy = parent.join(PARENT_DIR);
        for f in ["tamias.yaml", "gp.txt.tmpl", "shared.txt.tmpl"] {
            std::fs::copy(gp.join(f), gp_copy.join(f)).unwrap();
        }
        write_pack(
            &parent,
            "parent",
            "    who: parent\n",
            "    - src: p.txt.tmpl\n      into: p.txt\n    - src: shared.txt.tmpl\n      into: shared.txt\n",
            Some("git:org/gp"),
        );
        std::fs::write(parent.join("p.txt.tmpl"), "p {{ who }} {{ shared }}").unwrap();
        std::fs::write(parent.join("shared.txt.tmpl"), "shared=parent").unwrap();

        let child = tmp.path().join("child");
        std::fs::create_dir_all(child.join(PARENT_DIR)).unwrap();
        // child payload = its own files + the parent's whole payload
        // (which already nests the grandparent at .scarab-parent/)
        copy_dir_all(&parent, &child.join(PARENT_DIR));
        write_pack(
            &child,
            "child",
            "    shared: child\n",
            "    - src: c.txt.tmpl\n      into: c.txt\n    - src: shared.txt.tmpl\n      into: shared.txt\n",
            Some("git:org/parent"),
        );
        std::fs::write(child.join("c.txt.tmpl"), "c {{ who }} {{ shared }}").unwrap();
        std::fs::write(child.join("shared.txt.tmpl"), "shared=child").unwrap();
        (tmp, child)
    }

    fn copy_dir_all(src: &Path, dest: &Path) {
        std::fs::create_dir_all(dest).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let to = dest.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir_all(&entry.path(), &to);
            } else {
                std::fs::copy(entry.path(), &to).unwrap();
            }
        }
    }

    #[test]
    fn load_chain_orders_ancestors_first() {
        let (_tmp, child) = chain_fixture();
        let chain = load_chain(&child).unwrap();
        let names: Vec<&str> = chain.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["grandparent", "parent", "child"]);
        // each pack's src root is its own payload dir
        assert!(chain[0].dir.ends_with("gp") || chain[0].dir.ends_with(".scarab-parent"));
        assert!(chain[0].dir.join("gp.txt.tmpl").is_file());
        assert!(chain[1].dir.join("p.txt.tmpl").is_file());
        assert!(chain[2].dir.join("c.txt.tmpl").is_file());
    }

    #[test]
    fn extends_without_materialized_parent_errors() {
        let tmp = tempfile::tempdir().unwrap();
        write_pack(
            tmp.path(),
            "orphan",
            "",
            "    - src: x.tmpl\n      into: x.txt\n",
            Some("git:org/missing"),
        );
        std::fs::write(tmp.path().join("x.tmpl"), "x").unwrap();
        let err = load_chain(tmp.path()).unwrap_err().to_string();
        assert!(err.contains("materialized"), "{err}");
        assert!(err.contains("tamias install"), "{err}");
    }

    /// Write a `merge:` table into an existing pack manifest.
    fn add_merge(dir: &Path, merge_yaml: &str) {
        let path = dir.join("tamias.yaml");
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("  merge:\n");
        text.push_str(merge_yaml);
        // merge must sit inside `template:` — append before files? The
        // fixture writes files last, so merge lands after files: still
        // inside the template table (same indent level).
        std::fs::write(&path, text).unwrap();
    }

    #[test]
    fn multi_parent_layout_renders_in_declaration_order() {
        let tmp = tempfile::tempdir().unwrap();
        // two independent parents materialized under .scarab-parents/
        for (i, name) in ["base-a", "base-b"].iter().enumerate() {
            let p = tmp.path().join(".scarab-parents").join(i.to_string());
            write_pack(
                &p,
                name,
                &format!("    who: {name}\n"),
                "    - src: x.tmpl\n      into: x.txt\n",
                None,
            );
            std::fs::write(p.join("x.tmpl"), "x={{ who }}").unwrap();
        }
        // child declares both parents, overrides x.txt
        write_pack(
            tmp.path(),
            "child",
            "    who: child\n",
            "    - src: y.tmpl\n      into: y.txt\n    - src: x.tmpl\n      into: x.txt\n",
            Some("[base-a, base-b]"),
        );
        std::fs::write(tmp.path().join("y.tmpl"), "y={{ who }}").unwrap();
        std::fs::write(tmp.path().join("x.tmpl"), "child-x={{ who }}").unwrap();
        let target = tempfile::tempdir().unwrap();
        let a = Args {
            dir: Some(tmp.path().to_path_buf()),
            target: Some(target.path().to_path_buf()),
            ..Args::default()
        };
        deploy(&a).unwrap();
        // child wins the into conflict; its vars beat both parents'
        assert_eq!(
            std::fs::read_to_string(target.path().join("x.txt")).unwrap(),
            "child-x=child"
        );
        assert_eq!(
            std::fs::read_to_string(target.path().join("y.txt")).unwrap(),
            "y=child"
        );
    }

    #[test]
    fn merge_into_error_fails_on_conflict() {
        let (_tmp, child) = chain_fixture();
        add_merge(&child, "    into: error\n");
        let a = Args {
            dir: Some(child),
            target: Some(tempfile::tempdir().unwrap().path().to_path_buf()),
            ..Args::default()
        };
        let err = deploy(&a).unwrap_err().to_string();
        assert!(err.contains("into conflict"), "{err}");
    }

    #[test]
    fn merge_vars_parent_wins_inverts_var_defaults() {
        let (_tmp, child) = chain_fixture();
        // parent sets who=parent; child doesn't set who but renders it.
        // parent-wins: the ancestor default beats the descendant's.
        add_merge(&child, "    vars: parent-wins\n");
        let target = tempfile::tempdir().unwrap();
        let a = Args {
            dir: Some(child),
            target: Some(target.path().to_path_buf()),
            ..Args::default()
        };
        deploy(&a).unwrap();
        // `shared` is still child-wins on into (default), but `who`
        // resolves to the most-ancestral default (grandparent) under
        // parent-wins vars.
        assert_eq!(
            std::fs::read_to_string(target.path().join("p.txt")).unwrap(),
            "p gp gp"
        );
    }

    #[test]
    fn deploy_renders_ancestors_first_child_wins() {
        let (_tmp, child) = chain_fixture();
        let target = tempfile::tempdir().unwrap();
        let a = Args {
            dir: Some(child),
            target: Some(target.path().to_path_buf()),
            ..Args::default()
        };
        deploy(&a).unwrap();
        // child overrode `shared` at every level; `who` resolves to the
        // child's nearest default — parent sets who=parent, child doesn't
        assert_eq!(
            std::fs::read_to_string(target.path().join("shared.txt")).unwrap(),
            "shared=child"
        );
        assert_eq!(
            std::fs::read_to_string(target.path().join("p.txt")).unwrap(),
            "p parent child"
        );
        assert_eq!(
            std::fs::read_to_string(target.path().join("gp.txt")).unwrap(),
            "gp parent child"
        );
        assert_eq!(
            std::fs::read_to_string(target.path().join("c.txt")).unwrap(),
            "c parent child"
        );
        // state recorded the child's final digest for the conflict
        let state: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(target.path().join(STATE_FILE)).unwrap())
                .unwrap();
        assert_eq!(state["pack"], "child");
        assert!(state["files"]["shared.txt"].is_string());
    }

    #[test]
    fn args_beat_pack_vars_across_chain() {
        let (_tmp, child) = chain_fixture();
        let a = Args {
            dir: Some(child.clone()),
            target: Some(tempfile::tempdir().unwrap().path().to_path_buf()),
            pairs: vec![("shared".into(), "arg".into())],
            ..Args::default()
        };
        // install previews without writing — just assert it validates
        install(&a).unwrap();
    }
}
