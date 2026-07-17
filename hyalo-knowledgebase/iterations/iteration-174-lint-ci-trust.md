---
title: >-
  Iteration 174 — lint & CI trust (parse errors surface, skip visibility, honest
  caps)
type: iteration
date: 2026-07-17
tags:
  - iteration
  - lint
  - ci
  - fix-wave
status: completed
branch: iter-174/lint-ci-trust
---

# Iteration 174 — lint & CI trust

## Goal

A green `hyalo lint` in CI must actually mean the vault is clean: files that
cannot be parsed become loud errors instead of silently vanishing, dropped
input paths are visible in every output format, and result caps never lie.
Fixes release blocker **RB-3** (lint half) and **UX-B** from
[[dogfood-results/dogfood-v0180-okf-profiles-pre-release]].

## Note from iter-173

No scope overlap with iter-173's generator-safety work — it fixed the
*generator* half of RB-3 (`okf index`/`okf log`/`madr toc` skip a malformed
concept with an `eprintln!` stderr warning and continue). This iteration
fixes the *lint* half: a malformed file must become a loud, error-severity
**lint violation** (`HYALO005`), not a silent stderr note. Don't reuse
iter-173's skip-and-warn mechanism here — a warning that lint swallows is
exactly the silent-drop bug this iteration exists to close.

Two things worth reusing from iter-173's implementation as precedent:

- **Stable violation-kind constants + per-violation `autofixable: Option<bool>`**
  on `InternalViolation` (`crates/hyalo-cli/src/commands/lint.rs`, see
  `VIOLATION_KIND_MISSING_REQUIRED_NO_DEFAULT`): `HYALO005` should follow the
  same shape (a `pub const VIOLATION_KIND_*` / stable rule id, not an inline
  string) rather than inventing a new pattern.
- **`root_cause(&anyhow::Error)` helper** in `crates/hyalo-cli/src/commands/okf.rs`
  (deepest error message in an anyhow chain, for a terse one-line message):
  reuse or lift it to a shared location instead of duplicating the chain-walk
  when rendering the `HYALO005` parse-error message.

Also carrying forward iter-172/173's process lesson: write Acceptance
Criteria as single-line bullets naming the backing test/symbol in backticks
up front (the `ac-fidelity-check` gate requires it) rather than adding it
after the fact.

## Tasks

### 1. Unparseable frontmatter = error (RB-3) [5/5]

- [x] New error-severity violation (stable id, e.g. `HYALO005` /
  `frontmatter-parse-error`) emitted by lint for any file whose frontmatter
  fails to parse (duplicate YAML keys, invalid YAML, oversized scalar…):
  file appears in output, counts in `files_checked`, message includes the
  parse error and line where known
- [x] `hyalo lint <file>` on such a file: exits 1 with the violation —
  never `0 files checked, no issues` (df-own-kb B3 repro)
- [x] Rule is listed in `lint-rules list`, severity-configurable but
  error by default; NOT downgradeable silently by profiles
- [x] Explicitly named `--file` arguments that are excluded by `[lint]
  ignore` print a notice instead of silently reporting 0 files checked
  (observed while linting the dogfood report — same silent-drop family)
- [x] Changelog + release-notes entry: this is an intentional behavior change
  (previously-invisible corrupt files now fail CI)

### 2. Skip visibility in text/github formats (UX-B) [2/2]

- [x] When any of `files_missing` / `files_skipped_non_md` /
  `files_skipped_outside_vault` is non-zero, `--format text` AND
  `--format github` print one summary line, e.g.
  `note: 13 input paths missing, 28 non-markdown skipped` (github: as a
  `::notice::`); JSON stays as-is
- [x] df-scale repro as e2e: 43-line diff list (15 .md, 13 missing, 28
  non-md) → the line appears in both formats with correct numbers

### 3. Honest caps and limits [3/3]

- [x] `lint --format json --detailed`: the 50-file `files[]` cap gets an
  override (honor `--limit`, with `--limit 0` = unlimited) and
  `files_truncated` stays accurate (mapl BUG-4)
- [x] Fix `--limit 0` returning an EMPTY file list on lint (documented as
  unlimited; `--count --limit 0` already correct) (ff-rdp B5)
- [x] `--format github` never truncates annotations (already true) — add a
  regression test asserting caps stay lifted

### 4. Fix-mode distinguishability [1/1]

- [x] `--format github` combined with `--fix --dry-run` marks would-be-fixed
  violations distinctly from remaining ones (e.g. `::notice` +
  `[fixable]` title prefix, summary line `N fixable, M remaining`) so the
  output is not identical to plain lint (df-own-kb U6)

### 5. Tests [4/4]

- [x] e2e: corrupt-frontmatter file → exit 1, HYALO005 in text/json/github
  outputs; full-vault run includes the file in counts
- [x] e2e: skip-summary lines in text + github with exact counters;
  absent when all inputs resolve
- [x] e2e: `--limit 0` unlimited on lint files; json detailed override;
  github + fix dry-run distinguishable output
- [x] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 6. Docs sync (same PR) [3/3]

- [x] README CI section: note the parse-error gate and skip-summary lines
  (they strengthen the PR-check story shipped in iter-170/171)
- [x] `lint --help` documents the new rule + `--limit 0` semantics
- [x] Retrospective task: adapt iteration-175 plan to what landed here

## Acceptance criteria

- [x] A vault containing one corrupt-frontmatter file can no longer produce a
  green CI lint run
- [x] The diff-aware pipeline from the README shows dropped-path counts in
  the job log without `--format json`
- [x] No lint output mode silently truncates or empties result lists
