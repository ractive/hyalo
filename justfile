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

# Run Miri against the parsing + unsafe surface of hyalo-core.
# Targets modules that don't touch the filesystem (Miri can't shim chmod/symlinks
# on macOS, which breaks tempfile-based tests). Covers the four `unsafe` blocks
# in scanner/strip.rs and the YAML/markdown parsers.
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
