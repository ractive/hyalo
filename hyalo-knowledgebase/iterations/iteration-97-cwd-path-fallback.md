---
title: "Iteration 97: Accept CWD-relative paths in --file"
type: iteration
date: 2026-04-06
tags:
  - iteration
  - ux
  - path-resolution
status: in-progress
branch: iter-97/cwd-path-fallback
---

# Iteration 97: Accept CWD-relative paths in --file

## Goal

When users pass `--file hyalo-knowledgebase/iterations/foo.md` (CWD-relative) instead of
`--file iterations/foo.md` (dir-relative), hyalo should silently strip the `dir` prefix and
resolve the file correctly, instead of failing with "not found".

## Design

In `resolve_file()`, after the current `dir.join(normalized)` fails:

1. Check if `normalized` starts with the `dir` prefix (as path components, not substring)
2. If so, strip the prefix and retry `dir.join(stripped)`
3. If both the original and stripped paths resolve to existing files (ambiguous case), prefer
   dir-relative (backwards-compatible) — no warning needed since the original succeeded

This only applies to relative paths — absolute paths remain rejected as today.

### Ambiguity analysis

The ambiguity occurs when `dir = "docs"` and the KB contains a `docs/` subfolder with a file
that also exists at the top level. E.g., both `docs/file.md` and `docs/docs/file.md` exist.
Then `--file docs/file.md` could mean either. We prefer the dir-relative interpretation
(current behavior) since it resolves first.

## Tasks

- [x] Add `strip_dir_prefix()` helper in `discovery.rs`
- [x] Integrate prefix stripping into `resolve_file()` as fallback
- [x] Add `resolve_file_args()` for early resolution in `find` dispatch
- [x] Add unit tests for the new path resolution (5 unit + 4 integration)
- [x] Add e2e tests covering CWD-relative paths (4 tests: find, nested find, set, ambiguity)
- [x] Run quality gates (fmt, clippy, test) — 530 tests pass
