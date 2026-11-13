#!/usr/bin/env bash
# scarab release — thin shim over tarsi-serve (the org release plane,
# project ADR-0005; repo config: ./tarsi.toml). One call runs
# plan → changelog → bump → tag → push; publish targets are off
# (ADR-0004: crates.io is tamias-only, GitHub Releases unused).
# Operator-run on main — `tarsi-serve release` is dry-run by
# default; --yes is the explicit opt-in.
#
# Pack-ref sync (`[release.sync_refs]` in tarsi.toml) rewrites the
# example pack manifest version inside the release commit, so the
# tagged tree carries consistent versions (the README's `version:`
# example is unrelated content — untouched). The pack publish
# itself stays manual (`tamias publish libraries/rust-repo`).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
# worktree-local env (gitignored); the service env file is canonical
[ -f ./.env ] && { set -a; source ./.env; set +a; }

tarsi-serve release --dir . --yes

echo "released: scarab <ver> (tag scarab-*-v<ver> per unit)"
echo "next: tamias publish libraries/rust-repo  # ship the example pack at <ver>"
