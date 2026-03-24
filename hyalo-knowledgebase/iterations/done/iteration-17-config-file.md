---
title: "Iteration 17: Per-Project Config File (.hyalo.toml)"
type: iteration
date: 2026-03-22
tags:
  - iteration
  - config
  - cli
  - ux
status: completed
branch: iter-17/config-file
---

# Iteration 17: Per-Project Config File (.hyalo.toml)

Add a per-project `.hyalo.toml` config file that sets defaults for CLI args (`dir`, `format`, `hints`), so users don't have to repeat flags on every invocation.

## Config format

```toml
# .hyalo.toml (in CWD)
dir = "./my-vault"   # default: "."
format = "text"      # default: "json"
hints = true         # default: false
```

## Tasks

### Config module (`src/config.rs`)

- [x] Add `toml = "0.8"` dependency to `Cargo.toml`
- [x] Create `src/config.rs` with `ConfigFile` (serde, `deny_unknown_fields`) and `ResolvedDefaults` structs
- [x] Implement `load_config()` — reads `.hyalo.toml` from CWD, graceful fallback on errors
- [x] Implement `load_config_from(dir)` — testable variant with explicit directory
- [x] Add `pub mod config` to `src/lib.rs`
- [x] Unit test: missing config returns defaults
- [x] Unit test: full config overrides all defaults
- [x] Unit test: partial config merges with defaults
- [x] Unit test: malformed TOML returns defaults with warning
- [x] Unit test: unknown fields returns defaults (deny_unknown_fields)
- [x] Unit test: invalid format value passed through (validation is caller's job)

### CLI integration (`src/main.rs`)

- [x] Change `Cli.dir` to `Option<PathBuf>` (remove `default_value`)
- [x] Change `Cli.format` to `Option<String>` (remove `default_value`)
- [x] Add `--hints` / `--no-hints` flag pair (replaces `Option<bool>`)
- [x] Load config at start of `main()`, merge with CLI args (CLI wins)
- [x] Update help text to mention `.hyalo.toml`
- [x] Inline hint context construction (removed `build_hint_context` function)
- [x] Only propagate CLI-explicit flags into drill-down hints (not config values)
- [x] Add CONFIG paragraph to `--help` long description
- [x] Add Configuration section to README.md

### E2E tests (`tests/e2e_config.rs`)

- [x] Config sets default format
- [x] CLI `--format` overrides config format
- [x] Config sets default dir
- [x] CLI `--dir` overrides config dir
- [x] Missing config uses hardcoded defaults
- [x] Malformed config warns on stderr, still succeeds
- [x] Config sets `hints = true`
- [x] CLI `--no-hints` overrides config hints

### Quality gates

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] Build release and dogfood

## Acceptance criteria

- `.hyalo.toml` in CWD sets defaults for `dir`, `format`, `hints`
- CLI args always override config values
- Missing config file is silent — no warning, no error
- Malformed config warns on stderr but does not abort
- Unknown fields in config warn (catches typos)
- All existing tests still pass
