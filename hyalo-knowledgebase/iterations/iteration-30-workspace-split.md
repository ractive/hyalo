---
branch: iter-30/workspace-split
date: 2026-03-23
status: completed
tags:
- iteration
- architecture
- workspace
title: 'Iteration 30: Workspace Split'
type: iteration
---

# Iteration 30: Workspace Split

## Goal

Split the single crate into a workspace with `hyalo-core` (library) and `hyalo-cli` (binary). At ~15K lines the project is at the threshold where this pays off through faster incremental builds, enforced boundaries, and future reusability.

## Tasks

### Workspace setup
- [x] Create `crates/hyalo-core/` with its own `Cargo.toml`
- [x] Create `crates/hyalo-cli/` with its own `Cargo.toml`
- [x] Convert root `Cargo.toml` to workspace manifest with `[workspace]` members

### Move core modules to hyalo-core
- [x] Move: `scanner.rs`, `frontmatter.rs`, `filter.rs`, `tasks.rs`, `links.rs`, `heading.rs`, `content_search.rs`, `discovery.rs`, `types.rs`
- [x] Create `crates/hyalo-core/src/lib.rs` with public module exports
- [x] Ensure `hyalo-core` has zero CLI dependencies (no `clap`, no `output`, no `hints`)

### Move CLI modules to hyalo-cli
- [x] Move: `main.rs`, `commands/`, `output.rs`, `hints.rs`, `config.rs`, `fs_util.rs`
- [x] Add `hyalo-core` as dependency of `hyalo-cli`
- [x] Update all `use` paths in CLI code to reference `hyalo_core::`

### Test migration
- [x] Keep e2e tests in `crates/hyalo-cli/tests/`
- [x] Move unit tests with their modules
- [x] Move benchmarks to appropriate crate (micro → core, vault → core or cli)

### Verify
- [x] `cargo build --workspace`
- [x] `cargo test --workspace`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] Update CI/release workflows if paths changed
- [x] Dogfood the new binary

## Risk

**Medium complexity, low risk.** The modules already have clean boundaries (no circular deps). The main work is mechanical (move files, update imports). The risk is in getting the test and benchmark harness paths right.

## Acceptance Criteria

- [x] Workspace builds successfully with two crates
- [x] `hyalo-core` compiles independently with no CLI dependencies
- [x] All existing tests pass unchanged (or with minimal path updates)
- [x] Binary name remains `hyalo`
- [x] All quality gates pass
