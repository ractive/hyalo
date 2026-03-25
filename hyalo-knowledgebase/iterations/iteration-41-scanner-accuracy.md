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

- [[backlog/text-search-skips-code-blocks-undocumented]] (medium)
- [[backlog/regex-case-sensitivity-flags-inert]] (low)
- [[backlog/heading-code-spans-parsed-empty]] (low)
- [[backlog/summary-count-discrepancy]] (low)

## Tasks

### Content search in code blocks
- [ ] Add `on_code_block_line()` callback to `FileVisitor` (or equivalent mechanism)
- [ ] `ContentSearchVisitor` receives and matches lines inside fenced code blocks
- [ ] Link extraction still skips code blocks (no regression)
- [ ] Task extraction still skips code blocks (no regression)
- [ ] E2e test: search term only inside a code block is found

### Heading code spans
- [ ] Identify where inline code spans are stripped during heading extraction
- [ ] Preserve code span text in heading output (e.g., `` ### `versions` `` → heading text "versions")
- [ ] E2e test with code-span headings

### Regex case sensitivity flags
- [ ] Verify whether `(?-i)` inline flag is passed through to the regex engine
- [ ] Fix so `(?-i)` makes search case-sensitive
- [ ] E2e test: case-sensitive search returns fewer results than default

### Summary count discrepancy
- [ ] Reproduce the off-by-one between `summary` and `properties summary`
- [ ] Identify root cause (malformed file handling? case sensitivity? different code paths?)
- [ ] Fix so both commands agree
- [ ] E2e test verifying consistency

### Quality gates
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`

## Acceptance Criteria

- [ ] Content search finds matches inside code blocks
- [ ] `(?-i)` regex flag enables case-sensitive search
- [ ] Code-span headings render correctly in sections output
- [ ] `summary` and `properties summary` counts agree
- [ ] All quality gates pass
