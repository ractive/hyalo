---
title: Iteration 179 — lint precision (code fences, line numbers, message polish)
type: iteration
date: 2026-07-18
status: completed
branch: iter-179/lint-precision
tags:
  - iteration
  - lint
  - mdlint
related:
  - "[[dogfood-results/dogfood-v0180-final-pre-release]]"
  - "[[iterations/iteration-174-lint-ci-trust]]"
---

# Iteration 179 — lint precision

## Goal

Lint findings point at real problems on the right lines with grammatical,
non-redundant messages. Kills the HYALO001 false-positive class that made
all 11 findings on full MDN wrong, and the counting/wording glitches from
[[dogfood-results/dogfood-v0180-final-pre-release]].

## Tasks

### 1. HYALO001 skips fenced code (BUG-5, MEDIUM)

- [x] `[]` inside fenced code blocks (and inline code spans) no longer
  fires HYALO001 — new shared `rules/code_fence.rs` (CommonMark §4.5 fence +
  §6.3 inline-span tracking); MDN repros covered by
  `no_violation_inside_fenced_code_block`,
  `no_violation_inside_tilde_fenced_code_block`,
  `violation_after_fenced_code_block_closes`,
  `no_violation_for_bare_bracket_in_inline_code`,
  `mdn_repro_truthy_glossary_reduce_regex`
- [x] Audit sibling HYALO rules for the same fenced-code blindness — HYALO002
  shared the blindness (literal `- [ ]` in a code sample); fixed with the same
  fence tracking, covered by `open_task_inside_fenced_code_is_ignored`.
  HYALO003/004 operate on frontmatter only (no body scan); HYALO005 is a
  parse-error marker — neither is affected

### 2. File-absolute line numbers (BUG-6, MEDIUM)

- [x] HYALO001 (and any rule reporting body-relative lines) reports
  file-absolute line numbers — `lint.rs` applies `find_body_line_offset` in
  `diag_to_violation` so every body diagnostic (MD*/HYALO001/HYALO002) is
  offset past frontmatter; the redundant body-relative line number was removed
  from the HYALO001 message. Verified manually: a checkbox on file line 8
  reports `line 8`
- [x] Cross-check the older "lint MD-rule line numbers are body-relative"
  finding — same family, now fixed by the single `to_file_line` translation.
  The OKF/changelog profile paths already passed `find_body_line_offset`; only
  the core body path was missing it

### 3. Severity display vs counts (BUG-17, LOW)

- [x] Text rendering and summary counts agree: `BodyViolation` now carries a
  per-violation `severity`, and `output.rs` labels each line from its own
  severity (not the folded group's), so an `error`-labelled line is always
  counted as an error. Group severity is max-across-members in both the main
  (`group_severity`) and legacy (`adapt_view_result_to_ext`) paths

### 4. Message polish (LOWs)

- [x] Summary pluralization: `(1 errors, 0 warnings)` → `(1 error, 0
  warnings)` in `output.rs`; the `hyalo summary` lint hint pluralizes too
- [x] `--files-from` hint: proper singular/plural for both the missing and
  outside-vault counters (`hints.rs` `files_from_hints`), matching the
  already-correct `skip_summary` note
- [x] HYALO005 double prefix: `terse_root_cause` strips the redundant
  `failed to parse YAML frontmatter: ` prefix so the shared
  `PARSE_ERROR_PREFIX` is not doubled; both HYALO005 emission sites route
  through it. Covered by `terse_root_cause_strips_redundant_yaml_prefix`
- [x] MD034 URL detector stops swallowing trailing Liquid `{%`/`{{` — new
  `trim_md034_liquid` in `engine.rs`; covered by
  `md034_fix_does_not_swallow_trailing_liquid_tag`,
  `trim_md034_liquid_leaves_clean_urls_untouched`
- [x] MD011 on literal regex text: kept error-level (autofix only rewrites a
  real reversed-link shape) and added a docs note in the SEVERITY_TABLE
  comment on the false-positive class + workaround
- [x] `changelog add` into an existing empty category inserts a blank line
  after the `### Heading` — covered by
  `add_into_empty_category_keeps_blank_after_heading`

### 5. Multiple positional files (LOW friction)

- [x] `hyalo lint a.md b.md` accepted — positional `FILE` is now `Vec<String>`
  (repeatable), threaded through dispatch/run; verified manually (both files
  linted, `2 files checked`)

### 6. Retrospective

- [x] Retrospective captured in this file (see below). No downstream planned
  iterations depend on lint output shape; nothing to re-scope

## Retrospective

- The BUG-6 body-relative line bug existed only on the *core* body-diagnostic
  path — the OKF, changelog, and skills profile runners already threaded
  `find_body_line_offset`. When adding a new line-reporting rule family,
  route it through the same offset helper.
- Fence/inline-code suppression is now shared (`rules/code_fence.rs`); any
  future line-based HYALO rule that scans the body should reuse it rather than
  re-deriving fence state.
- `terse_root_cause` is the single choke-point for user-facing frontmatter
  parse messages; prefix/suffix noise stripping belongs there, not at each
  call site.

## Acceptance Criteria

- [x] `hyalo lint --rule HYALO001` reports zero findings on prose that
  documents `[]` in code (the 11 MDN false positives) — locked by the
  `mdn_repro_*` / fenced-code / inline-code tests in `hyalo001.rs`
- [x] Reported line numbers verified file-absolute — `to_file_line` offset in
  `lint.rs`, HYALO001 message de-duplicated, manual check confirms `line 8`
  for a checkbox on file line 8
- [x] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean (full
  workspace green; all four xtask check-* gates pass)
