#!/usr/bin/env bash
# Read-only release helper for the /release skill. Two modes:
#
#   release-preflight.sh check X.Y.Z   # run all preflight checks, PASS/FAIL per line, exit 1 on any FAIL
#   release-preflight.sh notes X.Y.Z   # emit the CHANGELOG section for X.Y.Z on stdout (post-rotation)
#
# Deliberately performs NO side effects: no commits, no tags, no gh release.
# The skill (SKILL.md) drives the mutating steps explicitly.
set -euo pipefail

MODE="${1:-}"
VER="${2:-}"
REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

usage() { echo "usage: $0 {check|notes} X.Y.Z" >&2; exit 2; }
[[ "$MODE" == "check" || "$MODE" == "notes" ]] || usage
[[ "$VER" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "error: version must be X.Y.Z (got '$VER')" >&2; exit 2; }

# ---------- notes mode ----------
if [[ "$MODE" == "notes" ]]; then
  # Section body from "## [X.Y.Z]" up to (excluding) the next "## [" heading.
  notes="$(sed -n "/^## \[${VER//./\\.}\]/,/^## \[/p" CHANGELOG.md | sed '$d')"
  if [[ -z "${notes//[[:space:]]/}" ]]; then
    echo "error: no non-empty '## [$VER]' section in CHANGELOG.md — rotate first (hyalo changelog release $VER --apply)" >&2
    exit 1
  fi
  printf '%s\n' "$notes"
  exit 0
fi

# ---------- check mode ----------
fail=0
ok()   { printf 'PASS  %s\n' "$1"; }
bad()  { printf 'FAIL  %s\n' "$1"; fail=1; }
note() { printf 'NOTE  %s\n' "$1"; }

# On main, clean, synced.
branch="$(git branch --show-current)"
[[ "$branch" == "main" ]] && ok "on main" || bad "on branch '$branch', not main"
[[ -z "$(git status --porcelain)" ]] && ok "working tree clean" || bad "working tree dirty"
git fetch -q origin main
if [[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]]; then
  ok "in sync with origin/main"
else
  bad "local main != origin/main (pull or push first)"
fi

# Version present in all three spots of the root Cargo.toml.
matches="$(grep -E "\"$VER\"" Cargo.toml | grep -c 'version' || true)"
if [[ "$matches" -ge 3 ]]; then
  ok "Cargo.toml version fields all read $VER ($matches matches)"
else
  bad "Cargo.toml has only $matches version fields at $VER — need workspace.package + hyalo-core + hyalo-mdlint"
fi

# Tag must not exist yet (local or remote).
if git rev-parse -q --verify "refs/tags/v$VER" >/dev/null; then
  bad "local tag v$VER already exists"
elif [[ -n "$(git ls-remote --tags origin "v$VER")" ]]; then
  bad "remote tag v$VER already exists"
else
  ok "tag v$VER does not exist yet"
fi

# Changelog state: pre-rotation ([Unreleased] non-empty) or post-rotation ([X.Y.Z] present).
unreleased="$(sed -n '/^## \[Unreleased\]/,/^## \[/p' CHANGELOG.md | sed '1d;$d')"
if grep -q "^## \[$VER\]" CHANGELOG.md; then
  ok "CHANGELOG has a [$VER] section (post-rotation)"
elif [[ -n "${unreleased//[[:space:]]/}" ]]; then
  ok "CHANGELOG [Unreleased] is non-empty (pre-rotation; run: hyalo changelog release $VER --apply)"
else
  bad "CHANGELOG has neither a [$VER] section nor [Unreleased] content — nothing to release"
fi

# Changelog grammar (needs a built binary; advisory when absent).
if [[ -x target/release/hyalo ]]; then
  if target/release/hyalo lint --dir . CHANGELOG.md --profile changelog --format text --no-hints 2>/dev/null | grep -q '(0 errors'; then
    ok "CHANGELOG lints clean under the changelog profile"
  elif target/release/hyalo lint --dir . CHANGELOG.md --profile changelog --format text --no-hints 2>/dev/null | grep -q 'no issues'; then
    ok "CHANGELOG lints clean under the changelog profile"
  else
    bad "CHANGELOG has changelog-profile lint errors (run the lint for details)"
  fi
else
  note "target/release/hyalo not built — skipping changelog-profile lint (cargo build --release)"
fi

# gh auth.
if gh auth status >/dev/null 2>&1; then ok "gh authenticated"; else bad "gh not authenticated"; fi

exit "$fail"
