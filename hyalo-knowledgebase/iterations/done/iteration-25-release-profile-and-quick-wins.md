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
- [x] Add `[profile.release]` to Cargo.toml: `lto = true`, `codegen-units = 1`, `strip = true`
- [x] Verify release binary size reduction (compare before/after)

### Dependency hygiene
- [x] Verify whether `tempfile` is used in production code paths (atomic writes) or only in tests; if only tests, move to `[dev-dependencies]`
- [x] Add `thiserror` dependency for structured error types

### Crate-level safety
- [x] Add `#![deny(unsafe_code)]` to `lib.rs` and `main.rs`
- [x] Change `pub` fields on `PropertyFilter` (`filter.rs:72`) to `pub(crate)`
- [x] Change `pub` fields on `Fields` struct (`filter.rs:332`) to `pub(crate)`
- [x] Change `pub fn extract_fence_language` (`scanner.rs:316`) to `pub(crate)`

### Small code smells
- [x] Remove dead `close_pos` variable in `tasks.rs:429` (currently suppressed with `let _ =`)
- [x] Convert `SectionFilter::parse` return type from `Result<Self, String>` to `anyhow::Result<Self>` (`heading.rs:65`)
- [x] Replace manual `Display + Error` impl on `FileResolveError` (`discovery.rs:164`) with `thiserror` derive
- [x] Store parsed K=V results after validation to remove `.expect("already validated")` calls in `append.rs:151`, `set.rs:195`, `remove.rs:217`

### Naming
- [x] Rename `outline.rs` → `sections.rs` (flagged in iter-12 review, not yet done)
- [x] Inline `fs_util.rs` (62 lines) into `discovery.rs` if no external callers

### Quality gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] Dogfood: `hyalo find --dir hyalo-knowledgebase` works with new binary

## Acceptance Criteria

- [x] Release binary is measurably smaller (strip) and uses LTO
- [x] `#![deny(unsafe_code)]` compiles clean
- [x] No `pub` fields on crate-internal structs
- [x] All `.expect("already validated")` removed from mutation commands
- [x] All quality gates pass
