---
title: "Dogfood bugfixes: derived title regex, text backlinks rendering"
type: iteration
date: 2026-03-30
tags:
  - bugfix
  - dogfood
status: completed
branch: iter-74/dogfood-bugfixes
---

## Goal

Fix the two bugs found during v0.6.0 post-refactor dogfooding on `docs/content` (3520 files) and `vscode-docs/docs` (339 files).

## Context

After iterations 70–73 (structural refactors), a thorough dogfood session confirmed zero regressions from the refactoring but uncovered two bugs in existing functionality.

## Bugs

### BUG 1: `--property 'title~=...'` doesn't match derived titles

**Repro:**
```
cd vscode-docs/docs
hyalo find --property 'title~=Settings' --format text   # → "No files matched"
hyalo find --fields title --limit 3 --format json        # → titles are present (derived from H1)
```

**Expected:** Files with derived title matching the regex should be returned.

**Root cause:** The property filter for `title` with regex operator `~=` only checks frontmatter properties, not the derived title (extracted from `# H1` heading). The `--sort title` and `--fields title` paths do consider derived titles, but the filter path does not.

**Fix:** In the property matching code, when the filter key is `title` and the file has no frontmatter `title` property, fall back to checking the derived title.

### BUG 2: Text format silently drops backlinks from `--fields backlinks`

**Repro:**
```
cd vscode-docs/docs
hyalo find --file configure/settings.md --fields backlinks --format text   # → shows only file path
hyalo find --file configure/settings.md --fields backlinks --format json   # → 116 backlinks
```

**Expected:** Text format should render backlinks similarly to how it renders `links` (source file, line number, label).

**Root cause:** The text formatter has rendering code for `links`, `sections`, `tasks`, `properties`, `tags`, and `matches` — but no code path for `backlinks`.

**Fix:** Add a backlinks rendering block to the text formatter, showing each backlink's source file, line, and label.

## Tasks

- [x] Fix derived title matching in property regex filter
- [x] Add backlinks rendering to text formatter
- [x] Add test: `--property 'title~=...'` matches derived title when no frontmatter title
- [x] Add test: `--format text` renders backlinks field
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
