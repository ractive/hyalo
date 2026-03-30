---
title: "Add --dry-run to set, remove, and append commands"
type: iteration
date: 2026-03-30
tags:
  - feature
  - ux
  - dogfood
status: planned
priority: 3
branch: iter-75/dry-run-mutations
---

## Goal

Add `--dry-run` support to the `set`, `remove`, and `append` mutation commands, matching the existing `--dry-run` on `mv`.

## Context

Found during v0.6.0 dogfooding (iteration 74). The `mv` command already supports `--dry-run` to preview changes before writing. The other mutation commands (`set`, `remove`, `append`) lack this, which is risky when applying bulk changes via `--glob`.

## Tasks

- [ ] Add `--dry-run` flag to `set` subcommand
- [ ] Add `--dry-run` flag to `remove` subcommand
- [ ] Add `--dry-run` flag to `append` subcommand
- [ ] When `--dry-run` is active, compute and display changes without writing to disk
- [ ] Add e2e tests for `--dry-run` on each mutation command
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
