---
title: "Add --dry-run to set, remove, and append commands"
type: iteration
date: 2026-03-30
tags:
  - feature
  - ux
  - dogfood
status: completed
priority: 3
branch: iter-75/dry-run-mutations
---

## Goal

Add `--dry-run` support to the `set`, `remove`, and `append` mutation commands, matching the existing `--dry-run` on `mv`.

## Context

Found during v0.6.0 dogfooding (iteration 74). The `mv` command already supports `--dry-run` to preview changes before writing. The other mutation commands (`set`, `remove`, `append`) lack this, which is risky when applying bulk changes via `--glob`.

## Tasks

- [x] Add `--dry-run` flag to `set` subcommand
- [x] Add `--dry-run` flag to `remove` subcommand
- [x] Add `--dry-run` flag to `append` subcommand
- [x] When `--dry-run` is active, compute and display changes without writing to disk
- [x] Add e2e tests for `--dry-run` on each mutation command
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
