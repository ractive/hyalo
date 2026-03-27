---
branch: iter-50/security-hardening
date: 2026-03-26
status: in-progress
tags:
- security
- pre-release
title: Security hardening for pre-release
type: iteration
---

# Security Hardening for Pre-Release

Pre-release security audit (2026-03-26) found **0 critical, 0 high, 3 medium, 6 low** issues.
Overall security posture is strong. This iteration addresses all actionable findings.

## Audit Tools Used

- `cargo audit` — 0 vulnerabilities in 153 dependencies
- `cargo deny check` — advisories ok, bans ok, licenses ok, sources ok
- Manual code review of all input parsing, path handling, unsafe blocks, and file I/O

## PII Scrub

- [x] Replace `/Users/james/...` paths with placeholders in `research/dogfooding-v0.4.1-dir-styles.md`
- [x] Replace `/Users/james/...` paths with placeholders in `research/dogfooding-v0.4.1-backlinks-mv.md`
- [x] Replace `/Users/james/...` paths with placeholders in `research/dogfooding-v0.4.1-consolidated.md`
- [x] Decide: rewrite git history to remove `james.bergamin@comparis.ch` author email, or accept it — **accepted, no rewrite**

## Code Hardening

- [x] Add per-line byte limit (~1 MB) in body scan phase of `scanner.rs` to prevent OOM on files with no newlines (MEDIUM)
- [x] Reject null bytes in `--file` path before normalization in `discovery.rs` (LOW — bypasses `.md` extension check within vault)
- [x] Add bounds check `if pid > i32::MAX as u32 { return false; }` before `pid as pid_t` cast in `index.rs:409` (LOW — tampered snapshot could prevent stale index cleanup)
- [x] Use `NamedTempFile::new_in(parent)` instead of predictable `.hyalo-index.tmp` in `index.rs:377-381` (LOW — minor symlink-attack vector)
- [x] Add `ensure_within_vault` assertion in `execute_plans` in `link_rewrite.rs:174-179` before writing (LOW — currently safe but fragile)

## Tooling & CI

- [ ] Run `cargo +nightly miri test` manually to verify unsafe blocks (not in CI — requires nightly, ~10-100x slower)
- [ ] Run `cargo-fuzz` manually on YAML frontmatter parsing and markdown link extraction (not in CI — open-ended runtime)
- [x] Document `--jq` infinite-loop risk in CLI help or README (informational)

## Verified Secure (no action needed)

- Path traversal: `has_parent_traversal()` + `dunce::canonicalize` + `starts_with` vault check
- Symlink escape: canonicalization correctly blocks out-of-vault reads
- YAML bombs: 8 KB / 200-line frontmatter cap prevents billion-laughs
- YAML code execution: `serde_yaml_ng` deserializes to `Value`, no tag constructors
- ReDoS: `regex` crate O(n) guarantee + 1 MiB compilation limit
- jaq injection: no filesystem, no env vars, no shell access in jaq
- No network I/O, no shell execution in production code
- Unsafe code (3 blocks): safety invariants manually verified as correct
- No `.unwrap()` on user-controlled data in production paths
- Atomic writes via `NamedTempFile::persist()` for frontmatter mutations

## Quality Gates

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
