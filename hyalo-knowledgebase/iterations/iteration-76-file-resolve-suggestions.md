---
title: "Improve --file error hints: detect directories, fuzzy-match file names"
type: iteration
date: 2026-03-30
tags:
  - ux
  - dogfood
status: planned
priority: 3
branch: iter-76/file-resolve-suggestions
---

## Goal

Improve the `--file` error messages in `resolve_file` (`hyalo-core/src/discovery.rs`) to give better suggestions when the target doesn't exist.

## Context

Found during v0.6.0 dogfooding (iteration 74). Currently `--file iterations` (a directory) suggests "did you mean iterations.md?" which is misleading. There's also no fuzzy matching for typos in file names.

A `levenshtein()` function already exists in `hyalo-cli/src/commands/summary.rs` (used for tag typo detection). It should be moved to a shared location and reused here.

## Tasks

- [ ] Move `levenshtein()` from `summary.rs` to a shared utility in `hyalo-core` (e.g. `hyalo-core::util::levenshtein`)
- [ ] In `resolve_file`, detect when the path is a directory and suggest `--glob 'dir/*'` instead of appending `.md`
- [ ] When the file is not found and not a directory, fuzzy-match against existing `.md` files in the same parent directory using Levenshtein distance
- [ ] Suggest the closest match(es) if distance is below a reasonable threshold
- [ ] Update `FileResolveError` variants as needed for the new hint types
- [ ] Add tests for directory detection hint
- [ ] Add tests for fuzzy file name suggestion
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
