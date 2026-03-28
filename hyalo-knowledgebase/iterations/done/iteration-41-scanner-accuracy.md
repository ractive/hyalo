---
branch: iter-41/scanner-accuracy
date: 2026-03-25
status: completed
tags:
- iteration
- scanner
- bug-fix
- consistency
title: Iteration 41 — Scanner Accuracy & Consistency
type: iteration
---

# Iteration 41 — Scanner Accuracy & Consistency

## Goal

Fix scanner inconsistencies found during dogfooding: content search missing code block lines, regex case flags not working, heading code spans parsed as empty, and summary/properties count discrepancies. These are small individually but together erode trust in hyalo's output.

## Backlog items

- [[backlog/done/text-search-skips-code-blocks-undocumented]] (medium)
- [[backlog/done/regex-case-sensitivity-flags-inert]] (low)
- [[backlog/done/heading-code-spans-parsed-empty]] (low)
- [[backlog/summary-count-discrepancy]] (low)

## Tasks

### Content search in code blocks
- [x] Add `on_code_block_line()` callback to `FileVisitor` (or equivalent mechanism)
- [x] `ContentSearchVisitor` receives and matches lines inside fenced code blocks
- [x] Link extraction still skips code blocks (no regression)
- [x] Task extraction still skips code blocks (no regression)
- [x] E2e test: search term only inside a code block is found

### Heading code spans
- [x] Identify where inline code spans are stripped during heading extraction
- [x] Preserve code span text in heading output (e.g., `` ### `versions` `` → heading text "versions")
- [x] E2e test with code-span headings

### Regex case sensitivity flags
- [x] Verify whether `(?-i)` inline flag is passed through to the regex engine
- [x] Fix so `(?-i)` makes search case-sensitive
- [x] E2e test: case-sensitive search returns fewer results than default

### Summary count discrepancy
- [x] Reproduce the off-by-one between `summary` and `properties summary`
- [x] Identify root cause (malformed file handling? case sensitivity? different code paths?)
- [x] Fix so both commands agree
- [x] E2e test verifying consistency

### Quality gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`

## Acceptance Criteria

- [x] Content search finds matches inside code blocks
- [x] `(?-i)` regex flag enables case-sensitive search
- [x] Code-span headings render correctly in sections output
- [x] `summary` and `properties summary` counts agree
- [x] All quality gates pass
