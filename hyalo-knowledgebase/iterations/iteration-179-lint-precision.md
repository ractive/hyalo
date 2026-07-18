---
title: "Iteration 179 — lint precision (code fences, line numbers, message polish)"
type: iteration
date: 2026-07-18
status: planned
branch: iter-179/lint-precision
tags: [iteration, lint, mdlint]
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

- [ ] `[]` inside fenced code blocks (and inline code spans) no longer
  fires HYALO001 — MDN repros (`glossary/truthy`, `array/reduce`,
  `regular_expressions/character_class`) become fixture tests
- [ ] Audit sibling HYALO rules for the same fenced-code blindness

### 2. File-absolute line numbers (BUG-6, MEDIUM)

- [ ] HYALO001 (and any rule reporting body-relative lines) reports
  file-absolute line numbers — offset by frontmatter length; the number
  embedded in the message text matches too
- [ ] Cross-check the older known finding "lint MD-rule line numbers are
  body-relative" (2026-07-10 review) — fix or verify already fixed, same
  family

### 3. Severity display vs counts (BUG-17, LOW)

- [ ] Text rendering and summary counts agree: a finding rendered
  `error SCHEMA` is counted as an error (typeless-concept repro showed
  two `error` lines but `1 errors, 2 warnings`)

### 4. Message polish (LOWs)

- [ ] Summary pluralization: `(1 errors, 0 warnings)` → `(1 error, 0
  warnings)` — and audit remaining count messages
- [ ] `--files-from` hint: `1 input path(s) did not exist` →
  proper singular/plural (hints.rs:330), matching the fixed note beside it
- [ ] HYALO005 double prefix: `could not parse frontmatter: failed to
  parse YAML frontmatter: ...` → single prefix
- [ ] MD034 URL detector stops swallowing trailing Liquid `{%` (GitHub
  Docs repro: fix would wrap template syntax into the autolink)
- [ ] MD011 on literal regex text: review the error-level default and/or
  add a docs note about the false-positive class
- [ ] `changelog add` into an existing empty category inserts a blank line
  after the `### Heading` (matches the section-creation path)

### 5. Multiple positional files (LOW friction)

- [ ] `hyalo lint a.md b.md` accepted (repeatable positional FILE),
  consistent with `--files-from` semantics

### 6. Retrospective

- [ ] Update remaining planned iterations with anything learned

## Acceptance Criteria

- [ ] `hyalo lint --rule HYALO001` on the MDN checkout reports zero
  findings (all 11 were false positives)
- [ ] Reported line numbers verified against raw files in e2e fixtures
- [ ] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean
