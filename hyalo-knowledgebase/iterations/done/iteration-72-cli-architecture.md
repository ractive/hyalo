---
title: "CLI architecture: index resolution, command trait, output pipeline, error handling"
type: iteration
date: 2026-03-30
tags:
  - refactor
  - architecture
  - ai-friendliness
status: completed
branch: iter-72/cli-architecture
---

## Goal

Eliminate repeated patterns in `run.rs` (1035 lines) by consolidating index resolution, introducing a command trait for dispatch, extracting the output pipeline, and centralizing error handling. Target: `run.rs` under 400 lines, each command self-contained.

## Context

After iteration 70 split the largest files into modules, `run.rs` still contains a 550-line match statement with 6 copies of index-or-scan boilerplate and a 77-line output pipeline. This iteration addresses the architectural patterns rather than file size.

## Sequencing

Changes must be done in order — each builds on the previous:

```
Change 1 (Index Resolution) → Change 2 (Command Trait) → Change 3 (Output Pipeline) → Change 4 (Error Handling)
```

## Tasks

### Change 1: Index Resolution Consolidation

- [x] Define `ResolvedIndex<'a>` enum in `commands/mod.rs` with `Snapshot(&'a SnapshotIndex)` and `Scanned(ScannedIndexBuild)` variants
- [x] Implement `as_index(&self) -> &dyn VaultIndex` on `ResolvedIndex`
- [x] Create `resolve_index()` function accepting snapshot ref, dir, file/glob args, format, site_prefix, needs_full_vault, and ScanOptions
- [x] Handle `build_scanned_index` error + `ScannedIndexOutcome` matching inside `resolve_index()`
- [x] Replace 6 index-or-scan blocks in `run.rs` (find, properties summary, tags summary, summary, backlinks, links fix) with `resolve_index()` calls
- [x] Keep per-command pre-filtering (e.g. `filter_index_entries` for properties/tags summary) in the match arm
- [x] Run full test suite, verify identical output

### Change 2: Command Dispatch Extraction

- [x] Define `CommandContext` struct bundling dir, site_prefix, format, snapshot_index, index_path
- [x] Extract command dispatch match block into `dispatch.rs` with `dispatch()` function
- [x] Move `parse_where_filters` into `dispatch.rs`
- [x] Convert validation `die()` calls in dispatch to return `Ok(CommandOutcome::UserError(...))` or `Err(e)`
- [x] Replace run.rs match block with `dispatch()` call
- [x] Keep `init` as early-return special case in run.rs
- [x] Run full test suite

### Change 3: Output Pipeline

- [x] Define `OutputPipeline` struct in `output_pipeline.rs` with user_format, jq_filter, hint_ctx, hints_active
- [x] Implement `OutputPipeline::finalize()` encapsulating jq filtering, hint generation, format conversion, error formatting
- [x] Replace run.rs output block with `pipeline.finalize(result)`
- [x] Run full test suite, especially jq and hints e2e tests

### Change 4: Error Handling Centralization

- [x] Define `AppError` enum with `User(String)`, `Internal(anyhow::Error)`, `Clap(clap::Error)`, `Exit(i32)` variants
- [x] Split `run()` into `run()` + `run_inner() -> Result<(), AppError>`
- [x] Convert all `die()` call sites to return `Err(AppError::...)`
- [x] Remove `die()` function
- [x] Handle `AppError` in `run()` with `warn::flush_summary()` on all paths
- [x] Preserve exit codes: User=1, Internal=2, Clap=clap's code
- [x] Run full test suite

### Quality gate

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace` — 498 passed
- [x] Verify `run.rs` is under 400 lines (398 lines)
