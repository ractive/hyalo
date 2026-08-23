---
title: Iteration 210 — output truth (lint/links JSON counters, hints, error parity)
type: iteration
date: 2026-08-23
status: completed
branch: iter-210/output-truth
tags:
  - iteration
  - json
  - lint
  - links
related:
  - "[[dogfood-results/dogfood-v0210-pre2-integrity-wave]]"
  - "[[iterations/iteration-204-dogfood-low-batch]]"
  - "[[iterations/iteration-208a-links-output-and-hint-ux]]"
---

# Iteration 210 — output truth (lint/links JSON counters, hints, error parity)

## Goal

Make machine-facing output tell the truth: lint JSON counters must
describe the whole run, an unmatched rule prefix must fail like an
unmatched rule id, the links text output must account for every broken
link, per-fix detail must be available to scripts, and every emitted
hint must actually work when copy-pasted.

## Context

From [[dogfood-results/dogfood-v0210-pre2-integrity-wave]]:

- **BUG-6 (MEDIUM, found independently by two testers)**: lint JSON
  `results.total` counts only listed files while `warnings`/`errors`
  count the whole run (MDN `web/api`: total 1,358 vs 14,248 — 10×);
  `rules_fired` is computed over the truncated set (7 vs 8); and
  `files_truncated` is derived from `files_checked > limit` instead of
  actual list truncation, false-positive on any vault over 50 files.
  The text renderer gets all of this right — only JSON lies.
- **BUG-5 (MEDIUM)**: `lint --rule-prefix nope` warns "matches no rule;
  nothing will be linted", then runs every MD rule anyway at exit 0.
  `--rule NOPE999` correctly exits 1.
- **UX-4**: `links` text output omits the `fuzzy` bucket, so
  `6098 broken` vs `25 fixable + 1400 unfixable` leaves 4,673 links
  unaccounted for on GitHub Docs; JSON reconciles exactly. The bucket is
  also absent from `links --help` OUTPUT.
- **BUG-11 (JSON aspect only)**: `links fix` JSON carries `fixes`,
  `fuzzy_fixes`, `case_mismatch_fixes` keys that are always empty
  arrays, even in dry-run — per-fix proposals cannot be audited
  programmatically before applying.
- **BUG-13 (hint/parity items)**: `lint sub/` emits hint
  `--glob 'sub//*'` whose double slash matches nothing at exit 0 — a
  copy-pasteable hint that reads as "clean"; the `did you mean X.md?`
  suggestion is emitted without checking the candidate exists
  (`nosuchdir/` → `nosuchdir/.md`); `find nosuchdir/` exits 0 with "No
  results" while `lint nosuchdir/` exits 1 not-found; and `links auto`
  JSON `col` is 1-based but **byte**-indexed and undocumented (char
  column 9 reported as 12 on a multibyte line).
- Also: `links`, `views`, and `lint-rules list` emit zero hints
  (dogfood UX-4) — `links` especially is a navigation dead end.

Note on `col`: if this iteration lands before v0.21.0 is cut, changing
`col` to Unicode-scalar columns (matching lint's `column`) costs no
extra breaking change beyond iter-204's already-documented one. If it
lands after the release, document byte semantics instead and defer the
switch. Decide by release status at implementation time and record the
decision.

## Tasks [10/10]

- [x] BUG-6: make lint JSON `total`, per-rule counts, and `rules_fired`
      describe the full run (matching `warnings`/`errors`), and compute
      `files_truncated` from actual list truncation. Add e2e asserting
      `total == warnings + errors` and `files_truncated == (listed <
      files_with_violations)` on a >50-file fixture vault.
- [x] BUG-5: an unmatched `--rule-prefix` exits 1 with the same error
      shape as an unmatched `--rule` (naming the prefix, hinting
      `lint-rules list`); matching prefixes unchanged.
- [x] UX-4: add the `fuzzy` bucket to `links` text output and to the
      OUTPUT section of `links --help`; assert text and JSON buckets sum
      to `broken` in an e2e.
- [x] BUG-11/JSON: populate per-fix detail (`fixes`, `fuzzy_fixes`,
      `case_mismatch_fixes`: file, line, col, from-target, to-target,
      strategy, confidence) in dry-run and apply JSON, or remove the
      always-empty keys — populate is strongly preferred; it unblocks
      programmatic review of fuzzy proposals.
- [x] BUG-13: fix the directory-hint glob (`sub/*`, not `sub//*`) and
      gate `did you mean X.md?` on the candidate existing; add both to
      the executed-hint e2e gate so emitted hints are run, not just
      string-checked.
- [x] BUG-13: unify missing-path behavior — `find <nonexistent path>`
      reports not-found at exit 1 like `lint`/`read` (L-7 parity).
- [x] `col` semantics: per the Context note, either switch to
      Unicode-scalar columns (pre-release) or document byte semantics in
      `links --help`; either way add a multibyte-line e2e pinning it.
- [x] Give `links`, `views`, and `lint-rules list` at least one useful
      drill-down hint each, correctly classified read-only vs `[writes]`.
- [x] (from superseded iter-208a) Redesign `links` text-output ordering so
      the actionable buckets (unfixable, out-of-vault, case mismatches)
      are not buried under thousands of per-link fix lines — decide the
      layout against real GitHub Docs/MDN output and record a DEC; surface
      `out_of_vault_links`/`unfixable_links` in text output when
      non-empty, not just JSON.
- [x] (from superseded iter-208a) De-duplicate hint listings that repeat
      the same long `--index-file <path>` 4-5 times; make the
      `[types.note]` config error suggest `hyalo types set` as the fix
      path.

## Acceptance criteria [7/7]

- [x] On a 61-file vault with 1 violating file: `total` equals the
      whole-run violation count, `files_truncated` is `false`, and
      `rules_fired` covers all firing rules at any `--limit`
- [x] `lint --rule-prefix nope` exits 1 and lints nothing
- [x] `links` text buckets sum to the broken count on the GitHub Docs
      scratch copy
- [x] A script can list every proposed fuzzy fix (file, targets,
      confidence) from dry-run JSON without parsing text output
- [x] Every hint emitted by `lint`, `find`, `links`, `views`, and
      `lint-rules list` executes successfully verbatim (extend the
      executed-hint gate)
- [x] A GitHub Docs scratch-copy `links fix` text run shows out-of-vault
      and unfixable counts without scrolling past the per-link fix lines
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Unifying the `.results` JSON shape across commands (dogfood UX-6) — a
  breaking re-envelope needs its own design pass; file it if it grows.
- Fuzzy confidence scoring itself —
  [[iterations/iteration-212-fuzzy-confidence-trust]].
