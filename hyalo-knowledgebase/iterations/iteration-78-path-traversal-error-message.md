---
title: "Improve error message for path traversal rejection"
type: iteration
date: 2026-03-30
tags:
  - ux
  - dogfood
status: planned
priority: 4
branch: iter-78/path-traversal-error-msg
---

## Goal

When `--file` rejects a path due to traversal (e.g. `../Cargo.toml`), say "path outside vault" instead of the generic "file not found".

## Context

Found during v0.6.0 dogfooding (iteration 74). `--file ../Cargo.toml` returns `Error: file not found` which is technically accurate (the file isn't found within the vault) but hides the real reason — the path was rejected because it escapes the vault boundary. The `FileResolveError::OutsideVault` variant already exists for symlink escapes but isn't used for `..` segment traversal, which falls through to `NotFound`.

## Tasks

- [ ] In `resolve_file`, return `OutsideVault` (or a new variant) when `..` segments are detected, instead of falling through to `NotFound`
- [ ] Update error message to clearly state the path resolves outside the vault
- [ ] Add/update test for `..` traversal error variant
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
