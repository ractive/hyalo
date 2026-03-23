---
branch: iter-25/release-profile-quick-wins
date: 2026-03-23
status: superseded
tags:
- iteration
- release
- code-quality
title: 'Iteration 25: Release Profile & Quick Wins'
type: iteration
---

# Iteration 25: Release Profile & Quick Wins

## Goal

Bundle all trivial, zero-risk fixes that improve binary quality, enforce safety, and clean up small code smells. These are the lowest-effort highest-value changes from the code review.

## Tasks

### Release profile
- [ ] Add `[profile.release]` to Cargo.toml: `lto = true`, `codegen-units = 1`, `strip = true`
- [ ] Verify release binary size reduction (compare before/after)

### Dependency hygiene
- [ ] Verify whether `tempfile` is used in production code paths (atomic writes) or only in tests; if only tests, move to `[dev-dependencies]`
- [ ] Add `thiserror` dependency for structured error types

### Crate-level safety
- [ ] Add `#![deny(unsafe_code)]` to `lib.rs` and `main.rs`
- [ ] Change `pub` fields on `PropertyFilter` (`filter.rs:72`) to `pub(crate)`
- [ ] Change `pub` fields on `Fields` struct (`filter.rs:332`) to `pub(crate)`
- [ ] Change `pub fn extract_fence_language` (`scanner.rs:316`) to `pub(crate)`

### Small code smells
- [ ] Remove dead `close_pos` variable in `tasks.rs:429` (currently suppressed with `let _ =`)
- [ ] Convert `SectionFilter::parse` return type from `Result<Self, String>` to `anyhow::Result<Self>` (`heading.rs:65`)
- [ ] Replace manual `Display + Error` impl on `FileResolveError` (`discovery.rs:164`) with `thiserror` derive
- [ ] Store parsed K=V results after validation to remove `.expect("already validated")` calls in `append.rs:151`, `set.rs:195`, `remove.rs:217`

### Naming
- [ ] Rename `outline.rs` → `sections.rs` (flagged in iter-12 review, not yet done)
- [ ] Inline `fs_util.rs` (62 lines) into `discovery.rs` if no external callers

### Quality gates
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Dogfood: `hyalo find --dir hyalo-knowledgebase` works with new binary

## Acceptance Criteria

- [ ] Release binary is measurably smaller (strip) and uses LTO
- [ ] `#![deny(unsafe_code)]` compiles clean
- [ ] No `pub` fields on crate-internal structs
- [ ] All `.expect("already validated")` removed from mutation commands
- [ ] All quality gates pass
