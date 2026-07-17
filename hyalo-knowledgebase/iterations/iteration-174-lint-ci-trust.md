---
title: Iteration 174 — lint & CI trust (parse errors surface, skip visibility, honest caps)
type: iteration
date: 2026-07-17
tags:
  - iteration
  - lint
  - ci
  - fix-wave
status: planned
branch: iter-174/lint-ci-trust
---

# Iteration 174 — lint & CI trust

## Goal

A green `hyalo lint` in CI must actually mean the vault is clean: files that
cannot be parsed become loud errors instead of silently vanishing, dropped
input paths are visible in every output format, and result caps never lie.
Fixes release blocker **RB-3** (lint half) and **UX-B** from
[[dogfood-results/dogfood-v0180-okf-profiles-pre-release]].

## Tasks

### 1. Unparseable frontmatter = error (RB-3)

- [ ] New error-severity violation (stable id, e.g. `HYALO005` /
  `frontmatter-parse-error`) emitted by lint for any file whose frontmatter
  fails to parse (duplicate YAML keys, invalid YAML, oversized scalar…):
  file appears in output, counts in `files_checked`, message includes the
  parse error and line where known
- [ ] `hyalo lint <file>` on such a file: exits 1 with the violation —
  never `0 files checked, no issues` (df-own-kb B3 repro)
- [ ] Rule is listed in `lint-rules list`, severity-configurable but
  error by default; NOT downgradeable silently by profiles
- [ ] Explicitly named `--file` arguments that are excluded by `[lint]
  ignore` print a notice instead of silently reporting 0 files checked
  (observed while linting the dogfood report — same silent-drop family)
- [ ] Changelog + release-notes entry: this is an intentional behavior change
  (previously-invisible corrupt files now fail CI)

### 2. Skip visibility in text/github formats (UX-B)

- [ ] When any of `files_missing` / `files_skipped_non_md` /
  `files_skipped_outside_vault` is non-zero, `--format text` AND
  `--format github` print one summary line, e.g.
  `note: 13 input paths missing, 28 non-markdown skipped` (github: as a
  `::notice::`); JSON stays as-is
- [ ] df-scale repro as e2e: 43-line diff list (15 .md, 13 missing, 28
  non-md) → the line appears in both formats with correct numbers

### 3. Honest caps and limits

- [ ] `lint --format json --detailed`: the 50-file `files[]` cap gets an
  override (honor `--limit`, with `--limit 0` = unlimited) and
  `files_truncated` stays accurate (mapl BUG-4)
- [ ] Fix `--limit 0` returning an EMPTY file list on lint (documented as
  unlimited; `--count --limit 0` already correct) (ff-rdp B5)
- [ ] `--format github` never truncates annotations (already true) — add a
  regression test asserting caps stay lifted

### 4. Fix-mode distinguishability

- [ ] `--format github` combined with `--fix --dry-run` marks would-be-fixed
  violations distinctly from remaining ones (e.g. `::notice` +
  `[fixable]` title prefix, summary line `N fixable, M remaining`) so the
  output is not identical to plain lint (df-own-kb U6)

### 5. Tests

- [ ] e2e: corrupt-frontmatter file → exit 1, HYALO005 in text/json/github
  outputs; full-vault run includes the file in counts
- [ ] e2e: skip-summary lines in text + github with exact counters;
  absent when all inputs resolve
- [ ] e2e: `--limit 0` unlimited on lint files; json detailed override;
  github + fix dry-run distinguishable output
- [ ] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 6. Docs sync (same PR)

- [ ] README CI section: note the parse-error gate and skip-summary lines
  (they strengthen the PR-check story shipped in iter-170/171)
- [ ] `lint --help` documents the new rule + `--limit 0` semantics
- [ ] Retrospective task: adapt iteration-175 plan to what landed here

## Acceptance criteria

- [ ] A vault containing one corrupt-frontmatter file can no longer produce a
  green CI lint run
- [ ] The diff-aware pipeline from the README shows dropped-path counts in
  the job log without `--format json`
- [ ] No lint output mode silently truncates or empties result lists
