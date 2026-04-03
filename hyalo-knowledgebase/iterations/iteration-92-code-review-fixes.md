---
title: "Iteration 92: Code Review Fixes"
type: iteration
date: 2026-03-31
tags:
  - iteration
  - quality
  - correctness
status: in-progress
branch: iter-92/code-review-fixes
---

# Iteration 92: Code Review Fixes

## Goal

Address findings from the full-codebase Rust code review (2026-03-31). Focus on correctness and error-handling issues first, then API design improvements.

## High Priority

- [ ] Use `atomic_write` in `link_rewrite.rs:execute_plans` instead of `std::fs::write` — crash mid-write can corrupt files
- [ ] Replace `serde_json::to_string_pretty(...).unwrap_or_default()` with `.context("failed to serialize")?` across all call sites (`mv.rs`, `backlinks.rs`, `read.rs`, `tags.rs`, `properties.rs`, `links.rs`, `output.rs`)

## Medium Priority

- [ ] Surface error when `--dir` path doesn't exist in `run.rs:502` instead of silently falling back from `canonicalize`
- [ ] Refactor `HintContext` construction — extract builder or `From<&Commands>` to eliminate ~200 lines of duplicated field assignment in `run.rs`
- [ ] Replace `Result<Result<ResolvedIndex, CommandOutcome>>` with a dedicated enum in `mod.rs:157`
- [ ] Introduce typed error enum for frontmatter parse vs I/O errors instead of `is_parse_error` heuristic in `frontmatter/mod.rs:12`
- [ ] Avoid full properties map clone in `find/mod.rs:163-181` — compare derived title directly without cloning
- [ ] Replace `Arc<Mutex<Vec>>` with `mpsc` channel for walk errors in `discovery.rs:14`

## Low Priority

- [ ] Replace `expect` with `?` in `dispatch.rs:307-310`
- [ ] Log warning for pre-1970 mtime fallback in `index.rs:640`
- [ ] Deduplicate `saturating_sub(1)` adjustment in `warn.rs` — store suppression count directly
- [ ] Avoid `to_vec()` clone in `read.rs:301` — use drain/truncate instead
- [ ] Add `rename_entry` method to `SnapshotIndex` to avoid double `rebuild_path_index` during `mv`
- [ ] Tighten `pub` to `pub(crate)` on result structs (`set.rs`, `remove.rs`, `append.rs`, `config.rs`)
- [ ] Remove dead `format` parameter in `mv.rs:104`
