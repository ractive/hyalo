---
title: "CLI architecture: index resolution, command trait, output pipeline, error handling"
type: iteration
date: 2026-03-30
tags:
  - refactor
  - architecture
  - ai-friendliness
status: planned
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

- [ ] Define `ResolvedIndex<'a>` enum in `commands/mod.rs` with `Snapshot(&'a SnapshotIndex)` and `Scanned(ScannedIndexBuild)` variants
- [ ] Implement `as_index(&self) -> &dyn VaultIndex` on `ResolvedIndex`
- [ ] Create `resolve_index()` function accepting snapshot ref, dir, file/glob args, format, site_prefix, needs_full_vault, and ScanOptions
- [ ] Handle `build_scanned_index` error + `ScannedIndexOutcome` matching inside `resolve_index()`
- [ ] Replace 6 index-or-scan blocks in `run.rs` (find, properties summary, tags summary, summary, backlinks, links fix) with `resolve_index()` calls
- [ ] Keep per-command pre-filtering (e.g. `filter_index_entries` for properties/tags summary) in the match arm
- [ ] Run full test suite, verify identical output

### Change 2: Command Trait

- [ ] Define `CommandContext` struct bundling dir, site_prefix, format, snapshot_index, index_path
- [ ] Define `Command` trait with `execute(&self, ctx: &mut CommandContext) -> Result<CommandOutcome>`
- [ ] Add `hint_source()` and `hint_context()` methods to trait with defaults
- [ ] Create command structs for all 14 commands (Find, Read, Summary, Backlinks, Set, Remove, Append, Mv, Properties, Tags, Task, Links, CreateIndex, DropIndex) — each implements `Command`
- [ ] Write `prepare()` function: `Commands` enum → validated command struct with parsed filters
- [ ] Move filter parsing (property filters, task filter, fields, sort, section filters, tag validation) into `prepare()` for FindCommand
- [ ] Move `parse_where_filters` into `prepare()` for Set/Remove/Append
- [ ] Move hint_ctx construction into the command structs
- [ ] Replace run.rs match block with `prepare()` + `cmd.execute()`
- [ ] Keep `init` as early-return special case
- [ ] Run full test suite

### Change 3: Output Pipeline

- [ ] Define `OutputPipeline` struct in `output.rs` with user_format, effective_format, jq_filter, hint_ctx
- [ ] Implement `OutputPipeline::new()` encapsulating the effective_format logic
- [ ] Implement `OutputPipeline::finalize()` encapsulating jq filtering, hint generation, format conversion, error formatting
- [ ] Replace run.rs output block with `pipeline.finalize(result)`
- [ ] Run full test suite, especially jq and hints e2e tests

### Change 4: Error Handling Centralization

- [ ] Define `AppError` enum with `User(String)`, `Internal(anyhow::Error)`, `Clap(clap::Error)` variants
- [ ] Change `run()` to return `Result<(), AppError>`
- [ ] Convert all `die()` call sites to return `Err(AppError::...)`
- [ ] Remove `die()` function
- [ ] Update `main.rs` to match on `run()` result with `warn::flush_summary()` on all paths
- [ ] Preserve exit codes: User=1, Internal=2, Clap=clap's code
- [ ] Run full test suite

### Quality gate

- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Verify `run.rs` is under 400 lines
