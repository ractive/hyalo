#!/usr/bin/env bash
# Local drift guard for the pi extension.
#
# The extension (pi-package/extensions/hyalo.ts, iter-237: single source of
# truth, also embedded into the release binary via include_str! and shipped
# to users via `hyalo init --pi` and `pi install git:github.com/ractive/hyalo`).
# pi's extension API evolves without notice; drift has broken the extension
# silently before (models fell back to bash, nobody noticed). This script
# guards against that drift — LOCALLY, not in CI (no pi/LLM in CI).
#
# Two layers:
#   1. Static type-check: compile the template against the installed pi
#      package's own .d.ts + typebox. Catches signature/shape drift
#      deterministically, no LLM involved.
#   2. Live e2e: run pi with --no-builtin-tools (bash/read/edit/write OFF,
#      extension tools ON) so the model MUST use the hyalo tool. A broken
#      tool cannot hide behind a bash fallback.
#
# Usage: ./pi-extension-e2e.sh [path-to-extension]
#        (default: pi-package/extensions/hyalo.ts)

set -euo pipefail

TEMPLATE="${1:-pi-package/extensions/hyalo.ts}"
TEMPLATE="$(cd "$(dirname "$TEMPLATE")" && pwd)/$(basename "$TEMPLATE")"
RUNTIME="$(dirname "$(dirname "$TEMPLATE")")/lib/hyalo-api.js"
REPO_ROOT="$PWD"

fail() { echo "FAIL: $*" >&2; exit 1; }

# Prefer this checkout's release binary: the guard validates the template
# against THIS tree's hyalo (config envelope shape, [pi] section, lint
# output). A PATH hyalo may be an older release that parses differently.
# The extension resolves `hyalo` from PATH at runtime; this only pins the
# binary under test.
if [[ -x "target/release/hyalo" ]]; then
    export PATH="$PWD/target/release:$PATH"
fi

# --- locate pi and its package directory -------------------------------
PI_BIN="$(command -v pi)" || fail "pi not found on PATH"
# Resolve through Homebrew/npm symlink chains to the real package dir.
PI_BIN="$(readlink -f "$PI_BIN" 2>/dev/null || echo "$PI_BIN")"
PI_PKG="$(dirname "$PI_BIN")"
while [[ "$PI_PKG" != "/" && ! -f "$PI_PKG/package.json" ]]; do
    PI_PKG="$(dirname "$PI_PKG")"
done
[[ -f "$PI_PKG/package.json" ]] || fail "could not locate pi package.json from $PI_BIN"
[[ -f "$PI_PKG/dist/index.d.ts" ]] || fail "pi package at $PI_PKG has no dist/index.d.ts"
echo "pi package: $PI_PKG ($(node -p "require('$PI_PKG/package.json').version"))"

[[ -f "$TEMPLATE" ]] || fail "template not found: $TEMPLATE"
[[ -f "$RUNTIME" ]] || fail "API runtime not found beside template: $RUNTIME"
[[ -f "${RUNTIME%.js}.d.ts" ]] || fail "API declaration not found beside runtime"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/hyalo-pi-check.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

check_pi_load() {
    local extension="$1" label="$2" response
    response="$(printf '%s\n' '{"type":"get_commands"}' | \
        pi --mode rpc --no-session --offline --no-extensions --no-skills \
            --extension "$extension")" || fail "$label: Pi failed to load extension"
    grep -q '"name":"hyalo-summary"' <<<"$response" \
        || fail "$label: registered commands missing after load: $response"
}

echo
echo "== [1/5] loading source and offline init extension layouts =="
check_pi_load "$TEMPLATE" "source package"
mkdir "$WORK/init"
(cd "$WORK/init" && "$REPO_ROOT/target/release/hyalo" init --pi --dir . >/dev/null)
[[ ! -e "$WORK/init/.pi/node_modules" ]] || fail "init --pi unexpectedly installed dependencies"
check_pi_load "$WORK/init/.pi/extensions/hyalo.ts" "fresh init --pi"
echo "source and fresh-init loading OK"

# --- layer 1: static type-check -----------------------------------------
echo
echo "== [2/5] type-checking template against installed pi types =="

mkdir -p "$WORK/node_modules/@earendil-works" "$WORK/extensions" "$WORK/lib"
ln -s "$PI_PKG" "$WORK/node_modules/@earendil-works/pi-coding-agent"
ln -s "$PI_PKG/node_modules/typebox" "$WORK/node_modules/typebox"

cp "$TEMPLATE" "$WORK/extensions/extension-hyalo.ts"
cp "$RUNTIME" "$WORK/lib/hyalo-api.js"
cp "${RUNTIME%.js}.d.ts" "$WORK/lib/hyalo-api.d.ts"
cat > "$WORK/tsconfig.json" <<'EOF'
{
  "compilerOptions": {
    "strict": true,
    "noEmit": true,
    "module": "nodenext",
    "moduleResolution": "nodenext",
    "target": "es2022",
    "skipLibCheck": true,
    "types": []
  },
  "files": ["extensions/extension-hyalo.ts"]
}
EOF

(cd "$WORK" && npm exec --package=typescript@5 -- tsc -p tsconfig.json) \
    || fail "template does not type-check against installed pi ($PI_PKG) — extension API drift"
echo "type-check OK"

# --- layer 2: live e2e with builtin tools disabled ----------------------
echo
echo "== [3/5] live e2e: forcing the hyalo tool (no bash fallback possible) =="
# Query must return a plain count. Vault contents change, but the term
# "iteration" always matches in hyalo's own knowledgebase and test vaults
# that run this script; --count output is a bare number.
OUT="$(pi -ne --no-builtin-tools -e "$TEMPLATE" -p \
    'Call the hyalo tool with subcommand "find" and args ["\"iteration\"", "--count"]. Reply with ONLY the number the tool returned, nothing else.')" \
    || fail "pi run failed (extension may not have loaded — check registerTool/registerCommand API)"

COUNT="$(tr -d '[:space:]' <<<"$OUT")"
[[ "$COUNT" =~ ^[0-9]+$ ]] || fail "expected a bare number from the tool call, got: $OUT"
[[ "$COUNT" -gt 0 ]] || fail "tool ran but returned count 0 — suspicious for term 'iteration'"

echo "e2e OK: hyalo tool returned count=$COUNT"

# --- layer 3: post-write lint guardrail ---------------------------------
#
# Verify the tool_result guardrail still fires: with only the `write` tool
# enabled, the model writes a deliberately non-conforming vault file; the
# extension must append hyalo lint findings to the write tool's result.
# Catches drift in BOTH the pi event API and hyalo's config/lint output
# shapes (e.g. the JSON envelope changing would break vault resolution).
echo
echo "== [4/5] guardrail e2e: lint findings appended to write result =="
GUARD_FILE="hyalo-knowledgebase/.pi-e2e-guard.md"
rm -f "$GUARD_FILE"
OUT="$(pi -ne -t write -e "$TEMPLATE" -p "Use the write tool to create $GUARD_FILE with exactly this content:
---
title: Guardrail test
---
body

Then report verbatim any lint warnings or extra notes that appeared in the write tool's result. Do not fix anything.")" \
    || fail "pi guardrail run failed"
rm -f "$GUARD_FILE"

if ! grep -q "hyalo lint" <<<"$OUT"; then
    fail "guardrail did not fire: expected hyalo lint findings in the write result, got: $OUT"
fi
echo "guardrail e2e OK: lint findings were appended to the write result"

# --- layer 4: typed-tool e2e ---------------------------------------------
#
# One forced call per typed tool (hyalo_find / hyalo_read / hyalo_set /
# hyalo_task) with --no-builtin-tools, asserting non-empty structured
# output. The model MUST use the named tool; a broken registration or a
# schema/exec drift shows up as an error string instead of real output.
# Uses a scratch file so the vault is left untouched.
echo
echo "== [5/5] typed-tool e2e: one forced call each (find/read/set/task) =="
SCRATCH="hyalo-knowledgebase/pi-e2e-scratch/pi-e2e-typed.md"
# The scratch file must live at a visible path inside the vault: hyalo skips
# hidden files AND hidden directories during find queries. Cleaned up below.
mkdir -p "$(dirname "$SCRATCH")"
rm -f "$SCRATCH"
cat > "$SCRATCH" <<'EOF'
---
title: Typed tool e2e
type: note
status: draft
tags:
  - e2e
---
# Typed tool e2e

## Tasks

- [ ] sample task
EOF

run_typed() {
    local label="$1" prompt="$2" needle="$3"
    local out
    out="$(pi -ne --no-builtin-tools -e "$TEMPLATE" -p "$prompt")" \
        || fail "typed tool '$label': pi run failed (tool may not be registered)"
    if ! grep -q "$needle" <<<"$out"; then
        fail "typed tool '$label': expected output containing '$needle', got: $out"
    fi
    echo "typed tool OK: $label"
}

run_typed hyalo_find \
    'Call the hyalo_find tool with property ["title~=Typed tool"] and countOnly true. Reply with ONLY the number returned.' \
    '^[[:space:]]*1[[:space:]]*$'
run_typed hyalo_read \
    'Call the hyalo_read tool with file "pi-e2e-scratch/pi-e2e-typed.md". Then reply with ONLY the exact heading text of its second heading, nothing else.' \
    'Tasks'
run_typed hyalo_set \
    'Call the hyalo_set tool with file "pi-e2e-scratch/pi-e2e-typed.md", property "status=review". Reply with ONLY the word DONE when it succeeded.' \
    'DONE'
grep -q 'status: review' "$SCRATCH" || fail "hyalo_set ran but did not persist status=review"
run_typed hyalo_task \
    'Call the hyalo_task tool with file "pi-e2e-scratch/pi-e2e-typed.md" and mode "all". Reply with ONLY the word DONE when it succeeded.' \
    'DONE'
grep -q '\[x\]' "$SCRATCH" || fail "hyalo_task ran but no checkbox toggled to [x]"
rm -rf "$(dirname "$SCRATCH")"
echo "typed-tool e2e OK: all four typed tools answered structured calls"

echo
echo "PASS: template is compatible with installed pi; generic + typed tools and lint guardrail work"
