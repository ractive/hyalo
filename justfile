# Hyalo task runner. Install just: https://github.com/casey/just

default:
    @just --list

# Standard quality gates (run before every commit/PR).
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace -q
    @just gates

# Every xtask gate CI's `quality-gates` job runs, in the same order.
gates:
    #!/usr/bin/env bash
    # Keep this list in sync with .github/workflows/ci.yml `quality-gates`.
    # xtask is built once and invoked directly rather than via
    # `cargo run -p xtask`, which can deadlock on the nested cargo lock when a
    # gate itself shells out to cargo; an absolute CARGO_MANIFEST_DIR lets
    # xtask find the workspace root and CARGO names the cargo for nested runs.
    # CI runs cargo deny via cargo-deny-action; locally:
    # cargo install cargo-deny && cargo deny check
    set -euo pipefail
    root="{{justfile_directory()}}"
    cargo build -q -p xtask
    export CARGO_MANIFEST_DIR="$root/crates/xtask"
    export CARGO="${CARGO:-$(command -v cargo)}"
    xtask="$root/target/debug/xtask"
    "$xtask" check-feature-fanout
    "$xtask" check-help-drift
    "$xtask" check-command-reference
    "$xtask" check-bundled-skills
    "$xtask" check-pi-package-sync
    "$xtask" check-ts-types
    npm --prefix "$root/npm/hyalo" ci --ignore-scripts
    "$xtask" check-pi-runtime
    "$xtask" check-codex-package
    "$xtask" check-jq-recipes
    "$xtask" check-mutation-journal
    "$xtask" check-typed-output

fmt:
    cargo fmt --all

# Refresh crate-local embedded assets from the canonical Codex plugin skills.
sync-codex-package:
    cargo run -p xtask -- sync-codex-package

# Run Miri against the parsing surface of hyalo-core to detect UB.
# Targets modules that don't touch the filesystem (Miri can't shim chmod/symlinks
# on macOS, which breaks tempfile-based tests). Covers the scanner, YAML
# frontmatter, BM25, link extraction, and other pure-logic parsers.
# Requires: rustup component add --toolchain nightly miri
miri:
    cargo +nightly miri setup
    MIRIFLAGS="-Zmiri-disable-isolation" \
        cargo +nightly miri test -p hyalo-core --lib -- --test-threads=1 \
            scanner:: frontmatter:: bm25:: links:: heading:: \
            filter:: content_search:: case_index::tests

# Run Miri against an arbitrary test filter, e.g.: just miri-filter scanner::strip
miri-filter FILTER:
    MIRIFLAGS="-Zmiri-disable-isolation" \
        cargo +nightly miri test -p hyalo-core --lib -- --test-threads=1 {{FILTER}}

# Run Miri across all hyalo-core lib tests (most filesystem tests are
# #[cfg_attr(miri, ignore)] or will fail — useful to inventory remaining gaps).
miri-all:
    cargo +nightly miri setup
    MIRIFLAGS="-Zmiri-disable-isolation" \
        cargo +nightly miri test -p hyalo-core --lib -- --test-threads=1

# Drift guard for the pi extension (pi-package/extensions/hyalo.ts).
# Run locally after touching the extension or after upgrading pi — NOT in CI
# (no pi / LLM access there). Layer 1 type-checks the extension against the
# installed pi package's own .d.ts; layer 2 runs pi with --no-builtin-tools so
# the model must use the hyalo tool (no silent bash fallback).
pi-extension:
    ./pi-extension-e2e.sh

# Sync the vendored pi-package copies (crates/hyalo-cli/templates/pi/) from
# the canonical top-level pi-package/. Run after editing any pi-package
# skill, extension, or package.json — `cargo run -p xtask -- check-pi-package-sync`
# fails CI until the copies match again.
sync-pi-package:
    cargo run -p xtask -- sync-pi-package

# Refresh the optional tidy helper and reference in all skill distributions.
sync-jev-assets:
    cargo run -p xtask -- sync-jev-assets
