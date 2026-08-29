---
type: iteration
title: Iteration 250 — mdbook-lint 0.16.1 bump, strip the MD047 CRLF override
date: 2026-08-29
status: completed
tags:
  - iteration
  - upstream
  - mdlint
branch: iter-250/mdlint-0161-workaround-strip
---

# Iteration 250 — mdbook-lint 0.16.1 bump, strip the MD047 CRLF override

## Goal

Upstream closed [joshrotenberg/mdbook-lint#495](https://github.com/joshrotenberg/mdbook-lint/issues/495)
(our 2026-08-23 report) with mdbook-lint#496, shipped in **0.16.1
(2026-08-27)**: MD047 now inserts the file's own terminator and counts
CRLF as one unit when detecting extra trailing blank lines. Maintainer's
note: MD047 was the last LF-centric rule. That makes `md047_fix` in
`crates/hyalo-mdlint/src/engine.rs` — documented as "the one exception
still open" in [[docs/upstream-mdbook-lint-reports]] — the last piece of
upstream compensation code we carry. Remove it. Context:
[[iterations/iteration-196-mdlint-workaround-strip]].

## Tasks

- [x] `cargo update -p mdbook-lint-core -p mdbook-lint-rulesets` to 0.16.1
      (`Cargo.toml` already says `"0.16"`; no manifest change). Commit
      `Cargo.lock`.
- [x] Delete `md047_fix` and its dispatch branch
      (`engine.rs` ~line 667: `if rule_id == "MD047" && body.contains("\r\n")`)
      so MD047 on CRLF bodies goes through the generic `convert_fix` path
      like every other rule. Reword the `convert_fix` doc comment (~line 768)
      that contrasts itself with "the CRLF gap `md047_fix` compensates for".
- [x] Keep every CRLF fixture and e2e test that exercised the override
      (`tests/e2e/lint.rs` MD047 convergence tests, `hyalo-mdlint` unit
      tests) — they become the regression check that upstream's fix holds
      through our CRLF-atomic offset translation. Add one case for a
      mixed-endings file (upstream: terminator of the line before EOF wins);
      if our old override disagreed, upstream's behaviour is the one to keep.
- [x] Verify on a real CRLF vault copy: `lint --fix --rule MD047` converges
      in one run on (a) missing final newline, (b) three trailing blank
      lines, (c) mixed endings; no bare `\n` introduced (`grep -c $'\r$'`
      unchanged except the intended line).
- [x] Docs: `docs/upstream-mdbook-lint-reports.md` — turn "One exception
      still open" into an outcome section ("all compensation removed in
      iter-250"); CHANGELOG `[Unreleased]` → Changed (dependency bump) and
      Removed (override); any README/skill text that mentions the CRLF
      exception.
- [x] Gates: fmt, clippy -D warnings, test --workspace -q, all xtask
      check-* gates; `cargo package -p hyalo-mdlint` still succeeds.

## Acceptance criteria

- [x] `grep -rn md047_fix crates/` is empty; `hyalo-mdlint` contains no
      rule-specific fix overrides.
- [x] All pre-existing CRLF MD047 tests pass against 0.16.1 with their
      expected outputs preserved (re-pointed from `md047_fix` to `lint_body`;
      one gained a second-pass convergence assertion).
- [x] Gates green.

## Non-goals

- Any other dependency bump.
- A release. This is patch-level (v0.21.1) whenever the next one is cut.

## Links

- [[docs/upstream-mdbook-lint-reports]]
- [[iterations/iteration-196-mdlint-workaround-strip]]
