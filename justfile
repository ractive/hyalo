# Hyalo task runner. Install just: https://github.com/casey/just

default:
    @just --list

# Standard quality gates (run before every commit/PR).
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace -q

fmt:
    cargo fmt --all

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
# Copies exactly the set the gate checks (skills/*/SKILL.md, extensions/*.ts,
# package.json). POSIX shell: on Windows run from Git Bash or WSL.
sync-pi-package:
    for d in pi-package/skills/*/; do \
      n=$(basename "$d"); mkdir -p "crates/hyalo-cli/templates/pi/skills/$n"; \
      cp "$d/SKILL.md" "crates/hyalo-cli/templates/pi/skills/$n/SKILL.md"; \
    done
    mkdir -p crates/hyalo-cli/templates/pi/extensions
    cp pi-package/extensions/*.ts crates/hyalo-cli/templates/pi/extensions/
    cp pi-package/package.json crates/hyalo-cli/templates/pi/package.json
