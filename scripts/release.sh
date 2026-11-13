#!/usr/bin/env bash
# scarab release — thin shim over tarsi-serve (the org release plane,
# project ADR-0005; repo config: ./tarsi.toml). One call runs
# plan → changelog → bump → tag → push; publish targets are off
# (ADR-0004: crates.io is tamias-only, GitHub Releases unused).
# Operator-run on main — `tarsi-serve release` is dry-run by
# default; --yes is the explicit opt-in.
#
# Preserved repo step: the example pack manifest version is synced
# to the new workspace version *before* the release commit lands,
# so the tagged tree carries consistent versions (the README's
# `version:` example is unrelated content — untouched). The pack
# publish itself stays manual (`tamias publish
# libraries/rust-repo`).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
# worktree-local env (gitignored); the service env file is canonical
[ -f ./.env ] && { set -a; source ./.env; set +a; }

ws_version() { grep -m1 '^version = "' Cargo.toml | cut -d'"' -f2; }

# Learn the version tarsi will bump to (all releasing units share one
# workspace version — empty output means "don't sync").
new="$(tarsi-serve plan --dir . | python3 -c '
import json,sys
d=json.load(sys.stdin)
if not d.get("ok"): sys.exit(0)
units=[u for u in d["result"]["plan"]["units"] if u.get("releases")]
vs={u["new_version"] for u in units}
print(vs.pop() if len(vs)==1 else "")
')"
old="$(ws_version)"
if [ -n "$new" ] && [ "$new" != "$old" ]; then
    sed -i "s/^version: \"${old}\"/version: \"${new}\"/" libraries/rust-repo/tamias.yaml
    git add libraries/rust-repo/tamias.yaml
    git commit -qm "chore(release): sync rust-repo@${new} version ref ahead of tarsi bump"
fi

tarsi-serve release --dir . --yes

echo "released: scarab ${new:-$old} (tag scarab-*-v${new:-$old} per unit)"
echo "next: tamias publish libraries/rust-repo  # ship the example pack at ${new:-$old}"
