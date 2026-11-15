#!/usr/bin/env python3
"""Generate the crate-map table inside reference/crates.md
(via the shared `cypera` tool — never hand-edit the marked region).

Thin wrapper over `cypera gen crate-map` — the per-repo python
implementation was retired in favor of the org-wide generator
(sibling clone `../cypera`, the shared successor to the per-repo
`docs/gen-*.py` scripts). The LAYER table below is the only
per-repo input: it rides `--layer` overrides so the crate map keeps
its layer vocabulary and ordering (cypera's canonical rank reproduces
every repo's ORDER table).

cypera resolution order:
  1. `cargo run -q --manifest-path <repo>/../cypera/Cargo.toml -p cypera`
  2. `cypera` on PATH

SCARAB_ROOT overrides the `..` worktree layout (detached CI checkouts),
the same convention the retired python script used.
"""
import os, pathlib, shutil, subprocess, sys

DOCS = pathlib.Path(__file__).parent
ROOT = pathlib.Path(os.environ.get("SCARAB_ROOT") or DOCS.parent)
OUT = DOCS / "reference/crates.md"

LAYER = {
    "scarab-config": "port",
    "scarab-template": "port",
    "scarab-config-file": "adapter",
    "scarab-config-toml": "adapter",
    "scarab-config-yaml": "adapter",
    "scarab-template-minijinja": "adapter",
    "tamias-driver-template": "driver",
    "scarab-testkit": "testkit",
    "scarab-xtask": "tooling",
}

def cypera():
    manifest = ROOT.parent / "cypera" / "Cargo.toml"
    if manifest.exists():
        return ["cargo", "run", "-q", "--manifest-path",
                str(manifest), "-p", "cypera", "--"]
    exe = shutil.which("cypera")
    if exe:
        return [exe]
    sys.exit("cypera not found — expected a ../cypera sibling clone "
             "or `cypera` on PATH")

args = (cypera() + ["gen", "crate-map", "--repo", str(ROOT),
                    "--out", str(OUT)])
for name, layer in LAYER.items():
    args += ["--layer", f"{name}={layer}"]
subprocess.run(args, check=True)
