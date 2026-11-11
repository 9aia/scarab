//! tamias-driver-template — the `type: template` content driver:
//! renders a template pack's files into declared targets.
//!
//! Verbs (the tamias driver contract — `--dir <payload>
//! --name <pkg> [--config <vars-file>] [key=value …]`):
//!
//! - `install` — validate the pack (manifest parses, every `src`
//!   exists and compiles, vars resolve) and preview the render
//!   plan. Writes nothing — the payload dir is the evictable fetch
//!   cache; rendered output belongs to `deploy`.
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
//! Var context precedence (later wins): pack `template.vars`
//! defaults → auto vars (`repo`, `user`) → `--config` file →
//! trailing `key=value` args → `$TEMPLATE_VAR_<NAME>` env.
//!
//! The engine is minijinja (`scarab-template-minijinja`) —
//! sandboxed, no filesystem/network access from templates.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use scarab_template::{PackTemplate, TemplateEngine, merge_context, safe_into, safe_src};
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

/// Var layers, later winning: pack `template.vars` defaults →
/// auto vars (`repo`, `user`) → `--config` file →
/// `$TEMPLATE_VAR_<NAME>` env → trailing `key=value` args.
fn resolve_vars(pack: &PackTemplate, a: &Args, work_dir: &Path) -> Result<serde_json::Value> {
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
    Ok(merge_context(
        &pack.vars,
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

/// `install` — validate the pack end-to-end (manifest, sources,
/// render with resolved vars) and report the plan. Writes nothing.
fn install(a: &Args) -> Result<()> {
    let dir = required_dir(a)?;
    let (pack, name) = load_pack(&dir)?;
    let engine = MinijinjaEngine::new();
    // Auto vars describe the destination repo, not the pack payload.
    let ctx_dir = target_dir(a)?;
    let ctx = resolve_vars(&pack, a, &ctx_dir)?;
    let rendered = render_pack(&pack, &dir, &ctx, &engine)?;
    println!(
        "template: {name} — {} file(s) validated (engine: {})",
        rendered.len(),
        engine.name()
    );
    for (into, _) in &rendered {
        println!("  {}", into.display());
    }
    Ok(())
}

/// `deploy` — render + write into the target dir.
fn deploy(a: &Args) -> Result<()> {
    let dir = required_dir(a)?;
    let (pack, name) = load_pack(&dir)?;
    let engine = MinijinjaEngine::new();
    let target = target_dir(a)?;
    // Auto vars describe the destination repo, not the pack payload.
    let ctx = resolve_vars(&pack, a, &target)?;
    let rendered = render_pack(&pack, &dir, &ctx, &engine)?;
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
}
