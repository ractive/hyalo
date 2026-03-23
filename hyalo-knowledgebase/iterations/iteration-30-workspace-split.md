---
branch: iter-30/workspace-split
date: 2026-03-23
status: planned
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
- [ ] Create `crates/hyalo-core/` with its own `Cargo.toml`
- [ ] Create `crates/hyalo-cli/` with its own `Cargo.toml`
- [ ] Convert root `Cargo.toml` to workspace manifest with `[workspace]` members

### Move core modules to hyalo-core
- [ ] Move: `scanner.rs`, `frontmatter.rs`, `filter.rs`, `tasks.rs`, `links.rs`, `heading.rs`, `content_search.rs`, `discovery.rs`, `types.rs`
- [ ] Create `crates/hyalo-core/src/lib.rs` with public module exports
- [ ] Ensure `hyalo-core` has zero CLI dependencies (no `clap`, no `output`, no `hints`)

### Move CLI modules to hyalo-cli
- [ ] Move: `main.rs`, `commands/`, `output.rs`, `hints.rs`, `config.rs`, `fs_util.rs`
- [ ] Add `hyalo-core` as dependency of `hyalo-cli`
- [ ] Update all `use` paths in CLI code to reference `hyalo_core::`

### Test migration
- [ ] Keep e2e tests in `crates/hyalo-cli/tests/`
- [ ] Move unit tests with their modules
- [ ] Move benchmarks to appropriate crate (micro → core, vault → core or cli)

### Verify
- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] Update CI/release workflows if paths changed
- [ ] Dogfood the new binary

## Risk

**Medium complexity, low risk.** The modules already have clean boundaries (no circular deps). The main work is mechanical (move files, update imports). The risk is in getting the test and benchmark harness paths right.

## Acceptance Criteria

- [ ] Workspace builds successfully with two crates
- [ ] `hyalo-core` compiles independently with no CLI dependencies
- [ ] All existing tests pass unchanged (or with minimal path updates)
- [ ] Binary name remains `hyalo`
- [ ] All quality gates pass
