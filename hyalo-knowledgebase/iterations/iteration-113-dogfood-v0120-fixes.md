---
title: Iteration 113 — Dogfood v0.12.0 Fixes
type: iteration
date: 2026-04-14
status: completed
branch: iter-113/dogfood-v0120-fixes
tags:
  - iteration
  - dogfooding
  - bug-fix
  - ux
related:
  - "[[dogfood-results/dogfood-v0120-multi-kb]]"
---

# Iteration 113 — Dogfood v0.12.0 Fixes

## Goal

Fix bugs and UX issues found during the v0.12.0 multi-KB dogfood session (MDN 14K files, GitHub Docs 3.5K files, own KB 234 files). See [[dogfood-results/dogfood-v0120-multi-kb]] for the full report.

## Bugs

### BUG-1: `--dir` config lookup for `types set` / `views set` (HIGH)

`types set` and `views set` always write to the CWD's `.hyalo.toml`, ignoring `--dir`. When working on an external KB (e.g., `--dir ../mdn/files/en-us/`), the type gets written to the wrong config file. Subsequently, `types show` with `--dir` can't find it. This was partially addressed in iter-108 but only for read paths.

**Fix**: When `--dir` is set, resolve `.hyalo.toml` relative to the `--dir` root (or its nearest ancestor), not CWD. Create the file if it doesn't exist.

### BUG-2: `types set` reorders TOML sections alphabetically (MEDIUM)

Running `types set` to add a single type re-serializes the entire `.hyalo.toml` with sections in alphabetical order and changes escape styles. This creates massive diffs (133 lines for a 1-line addition).

**Fix**: Switch from `toml` (serialize/deserialize) to `toml_edit` for in-place, order-preserving TOML modifications.

### BUG-3: Bare boolean operator queries return 0 results silently (MEDIUM)

`find "and"` and `find "or"` return 0 results with no warning because the word is consumed as a boolean operator, leaving an empty query.

**Fix**: After BM25 query parsing, if the effective query is empty but the raw input was non-empty, emit a warning like: `warning: "and" was interpreted as a boolean operator, leaving an empty query. To search for the literal word, quote it: '"and"'`

### BUG-4: `task toggle --all` misses deeply indented checkboxes (MEDIUM)

`--all` uses a regex that requires checkboxes at 0–8 spaces of indentation. Checkboxes at 16 spaces (common in nested lists) are missed, but `--line N` toggles them fine because it uses a different, more permissive pattern.

**Fix**: Unify the checkbox detection regex between `--all`/`--section` and `--line`. Allow any indentation level.

### BUG-5: `create-index --index=PATH` ignores custom path (LOW)

The `--index=/tmp/mdn-hyalo-index` argument is silently ignored; the index is always written to `<vault>/.hyalo-index`.

**Fix**: Respect the `--index=PATH` value in `create-index`. If the path is relative, resolve it from CWD (not vault dir).

### BUG-8: `remove --tag` can't remove malformed comma-tags (LOW)

`remove --tag "cli,ux"` fails with "invalid character ',' in tag name". This means malformed comma-tags can only be fixed by editing files manually. Also, `append --property tags=` (empty value) silently adds `- ""` to the tags list.

**Fix**: Allow `remove --tag` to remove any exact tag string (skip validation for removal). Make `append --property tags=` (empty value) error instead of inserting an empty string.

## UX Improvements

### UX-2: `lint --type` shortcut (LOW)

Add `--type <name>` flag to `lint` that expands to `--glob <filename-template>` from the type's schema. So `hyalo lint --type iteration` would be equivalent to `hyalo lint --glob "iterations/iteration-*.md"`.

### UX-3: Lint should detect comma-joined tags (LOW)

Tags like `"cli,ux"` are malformed — they should be separate list items. Lint should warn about tags containing commas, and `--fix` should split them.

### UX-4: `task toggle --dry-run` (LOW)

All other mutation commands support `--dry-run`. Add it to `task toggle` for consistency.

## Data Quality (done during dogfood session)

- [x] Fixed 13 files in `backlog/done/` with comma-joined tags — split into proper YAML lists
- [x] Fixed CLAUDE.md reference to non-existent `--optional` flag for `types set`

## Tasks

### BUG-1: `--dir` config lookup
- [x] Add config resolution logic: when `--dir` is set, look for `.hyalo.toml` in `--dir` root first, then walk up ancestors
- [x] `types set` / `views set` write to the resolved config path
- [x] Create `.hyalo.toml` at `--dir` root if it doesn't exist and a write is needed
- [x] Add e2e tests: `types set --dir /tmp/test-kb/` creates config at `/tmp/test-kb/.hyalo.toml`
- [x] Add e2e tests: `views set --dir /tmp/test-kb/` same behavior

### BUG-2: Order-preserving TOML writes
- [x] Replace `toml::to_string` with `toml_edit` for `.hyalo.toml` mutations
- [x] Preserve section order, key order, comments, and escape styles
- [x] Add test: round-trip a `.hyalo.toml` through `types set` and verify minimal diff

### BUG-3: Empty BM25 query warning
- [x] After BM25 query parsing, check if effective query is empty but raw input was non-empty
- [x] Emit warning to stderr with suggestion to quote the literal word
- [x] Add test for `find "and"`, `find "or"`, `find "not"`

### BUG-4: Indentation-agnostic checkbox regex
- [x] Unify checkbox detection regex for `--all`/`--section` and `--line`
- [x] Support any indentation level (0+ spaces/tabs)
- [x] Add test with checkboxes at 0, 4, 8, 16 spaces

### BUG-5: `create-index --index=PATH`
- [x] Parse and respect the `--index=PATH` value in `create-index`
- [x] Resolve relative paths from CWD
- [x] Add test: `create-index --index=/tmp/test.idx` writes to `/tmp/test.idx`

### BUG-8: Malformed tag handling
- [x] Skip tag name validation in `remove --tag` (allow removing any exact string)
- [x] Error on `append --property tags=` (empty value) instead of inserting `""`
- [x] Add tests for both edge cases

### UX improvements
- [x] Add `lint --type <name>` flag that expands filename-template to `--glob`
- [x] Add comma-tag detection to lint (warn severity)
- [x] Add `--fix` support for splitting comma-tags into list items
- [x] Add `--dry-run` flag to `task toggle`

### Quality gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace -q`

## Acceptance Criteria

- [x] `hyalo types set foo --required title --dir /tmp/test-kb/` writes to `/tmp/test-kb/.hyalo.toml`
- [x] `hyalo types set` on an existing `.hyalo.toml` produces a minimal diff (no section reordering)
- [x] `hyalo find "and"` emits a warning about the bare boolean operator
- [x] `hyalo task toggle file.md --all` finds checkboxes at any indentation level
- [x] `hyalo create-index --index=/tmp/test.idx` writes index to the specified path
- [x] `hyalo lint` warns about tags containing commas
- [x] `hyalo task toggle file.md --line 5 --dry-run` previews without writing

**Superseded by [[iterations/iteration-118-split-index-flag]]:** `--index=PATH` is now `--index-file=PATH`; bare `--index` is a boolean flag.
