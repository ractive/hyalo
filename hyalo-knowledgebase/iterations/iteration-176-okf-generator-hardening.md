---
title: >-
  Iteration 176 — OKF generator hardening (marker safety, valid links, robust
  apply)
type: iteration
date: 2026-07-18
status: completed
branch: iter-176/okf-generator-hardening
tags:
  - iteration
  - okf
  - generators
  - data-safety
related:
  - "[[dogfood-results/dogfood-v0180-final-pre-release]]"
  - "[[iterations/iteration-173-generator-safety]]"
---

# Iteration 176 — OKF generator hardening

## Goal

Close the remaining data-safety and output-correctness holes in
`okf index` / `okf log` found by the final pre-release dogfood
([[dogfood-results/dogfood-v0180-final-pre-release]]). Two of the three
recommended pre-release fixes live here: **BUG-3** (marker-edge data loss)
and **BUG-10** (CommonMark-invalid generated links). After this iteration
the documented contracts hold unconditionally: "apply twice is a no-op",
"a per-file problem never aborts the run", and "the generated list is a
valid Markdown link list".

## Context

Iter-173 made adopt non-destructive for the happy paths; the dogfood found
the edges: dangling/reversed/duplicate `<!-- okf:index:begin/end -->`
markers corrupt on the second apply, generated links break CommonMark for
spaced paths and `]` in titles, and one unwritable target aborts a whole
apply mid-run leaving partial state.

## Tasks

### 1. Marker-edge safety (BUG-3, MEDIUM-HIGH, data loss)

- [x] Detect dangling begin (no end), dangling end (no begin), reversed
  (end before begin), and duplicate marker pairs during region scan
- [x] On any malformed-marker file: warn with the file path and marker
  problem, skip the file (never half-adopt, never rewrite across a
  dangling marker), count it in the run summary
- [x] `--dry-run` reports malformed-marker files the same way and still
  exits correctly for CI (drift vs clean unaffected by skipped files)
- [x] e2e: dangling-begin file survives two `--apply` runs byte-identical
  (the dogfood repro that previously deleted hand prose)
- [x] Duplicate marker pairs: warn that only the first region is managed
  (dogfood UX note) or include in the malformed-marker skip — decide and
  document

### 2. CommonMark-valid generated links (BUG-10, MEDIUM)

- [x] Wrap destinations containing spaces (or other link-breaking chars)
  in `<...>` or percent-encode them — `* [Title](<blocks table.md>)`
  renders as a link on GitHub
- [x] Escape `[` / `]` in link-text titles
- [x] Collapse newlines in `description` values to a single space before
  emitting the `- description` suffix
- [x] Same treatment for `Subdirectories` entries (`spaced dir/index.md`)
- [x] e2e: unicode + spaced filenames, `]` in title, multiline description
  all produce CommonMark-valid single-line bullets

### 3. Robust apply (BUG-11, MEDIUM)

- [x] A per-file write failure (e.g. `index.md` is a directory or
  unwritable) warns and continues; failed targets listed in the summary;
  exit code reflects partial failure without aborting remaining files
- [x] `--dry-run` detects impossible targets (existing directory named
  `index.md`/`log.md`) and reports them instead of claiming `create`
- [x] Same continue-on-error behavior for `okf log` directory targets

### 4. Scope and target validation (BUG-13, BUG-15)

- [x] `okf index <dir>` errors (exit 1) when the scope directory does not
  exist — no more vacuous `0 scanned` CI pass
- [x] `okf log <dir>` dry-run and apply agree for a nonexistent directory:
  either both create it or both reject with a clean message (decide;
  rejecting with a hint to create it is the conservative default)

### 5. Message and flag polish (BUG-12 + LOWs)

- [x] `-q`/`--quiet` suppresses the okf skip warnings (help already
  promises "suppress all warnings printed to stderr")
- [x] `okf log` multiline `--message`: indent continuation lines under the
  bullet (or reject newlines) so the log structure stays valid (BUG-14);
  `OKF-LOG-STRUCTURE` should flag a corrupted log either way
- [x] Grammar: `1 file wrote` → `1 file written`; `preserving 1 existing
  lines` → `1 existing line`
- [x] `okf log --action ""` errors like `--message ""` does (consistency)
- [x] `init --profile <p>` re-run prints an "unchanged" message when the
  result is byte-identical instead of `updated .hyalo.toml`

### 6. Marker-hygiene lint rule (enhancement from the report)

- [x] New okf-profile lint rule (e.g. `OKF-INDEX-MARKERS`) flagging
  dangling/reversed/duplicate managed-region markers so CI surfaces the
  precondition instead of the generator meeting it at apply time
- [x] `[okf] ignore` globs also exempt files from the okf lint rules
  (dogfood split-brain: `_template/**` excluded from generation but still
  lint-flagged) — **deliberate decision: deferred, not done this iteration.**
  `[okf] ignore` is not threaded into the lint pipeline
  (`lint_files_extended` → per-file loop) at all; wiring the ignore globset and
  vault-relative path through the whole lint machinery is a cross-cutting change
  disproportionate to this data-safety iteration. The `[lint] ignore` /
  `[schema] exempt` globs already cover the common `_template/**` case for the
  schema pass; the OKF advisory rules are warn-only (never fail CI), so the
  split-brain is cosmetic, not a gate. Tracked for a future lint-scoping
  iteration. See [[decision-log]].

### 7. Retrospective

- [x] Update remaining planned iterations with anything learned here
  (especially [[iterations/iteration-177-okf-docs-truth]], which documents
  the behavior this iteration finalizes)

## Acceptance Criteria

- [x] All dogfood repros from BUG-3/10/11/12/13/14/15 pass as e2e tests
- [x] `okf index --apply` twice is byte-idempotent on every fixture,
  including malformed-marker ones
- [x] No hand-written prose is ever deleted by the generator
- [x] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean
