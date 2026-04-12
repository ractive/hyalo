---
title: "Iteration 97: Accept CWD-relative paths in --file"
type: iteration
date: 2026-04-06
tags:
  - iteration
  - ux
  - path-resolution
status: completed
branch: iter-97/cwd-path-fallback
---

# Iteration 97: Accept CWD-relative paths in --file

## Goal

When users pass `--file hyalo-knowledgebase/iterations/foo.md` (CWD-relative) instead of
`--file iterations/foo.md` (dir-relative), hyalo should silently strip the `dir` prefix and
resolve the file correctly, instead of failing with "not found".

## Design

Treat the `dir` prefix as normalization — like stripping `./`. In `resolve_file()`,
`strip_dir_prefix()` removes the prefix unconditionally during normalization, before
any existence checks. One code path, no branching.

For `find`, the `filter_index_entries` function matches `--file` args by string equality
against index entries. A lightweight `strip_dir_prefix` call in the dispatch normalizes
the args before they reach the filter.

Only relative paths are affected — absolute paths remain rejected as before.

### Ambiguity

When `dir = "kb"` and the vault contains both `note.md` and `kb/note.md`, passing
`--file kb/note.md` resolves to `note.md` (the prefix is always stripped). This edge case
requires a subdirectory named identically to the vault dir, which is extremely unlikely.

## Tasks

- [x] Add `strip_dir_prefix()` helper in `discovery.rs`
- [x] Strip prefix in `resolve_file()` during normalization
- [x] Strip prefix in `find` dispatch for `filter_index_entries`
- [x] Add unit tests (5 for strip_dir_prefix, 4 for resolve_file)
- [x] Add e2e tests (find, nested find, set, ambiguity)
- [x] Update help text (PATH RESOLUTION in `--help`)
- [x] Run quality gates — 530 tests pass
