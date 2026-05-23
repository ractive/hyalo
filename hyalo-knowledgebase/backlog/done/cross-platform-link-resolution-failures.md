---
title: Cross-platform link-resolution tests fail on Linux + Windows
type: backlog
date: 2026-05-23
status: completed
priority: high
origin: PR
tags:
  - links
  - cross-platform
  - testing
  - case-sensitivity
---

## Problem

Two link-resolution e2e tests have been failing on `ubuntu-latest` and
`windows-latest` since at least PR #157 (iter-136). They pass on
`macos-latest` and locally on macOS. PR #157 was merged anyway; PR #158
inherited the same failures.

### Failing tests

1. `links::case_insensitive_links_fix_dry_run_reports_case_mismatches`
   - Setup: on-disk `iteration_protocols.md`, wikilink `[[Iteration_Protocols]]`,
     `.hyalo.toml` forces `[links] case_insensitive = "true"`.
   - Expected: `strategy = "LinkCaseMismatch"`
   - Actual on Linux: `strategy = "ShortFormStemMismatch"`
   - Likely cause: the new short-form-stem detection (iter-136) is taking
     precedence over the case-mismatch classification when both apply.
     The test was written assuming case-mismatch wins; the classifier
     order needs review.

2. `mv::mv_bare_wikilink_no_broken_links_after_move`
   - Setup: `a.md` contains `[[b]]`, then `mv b.md → archive/b.md`.
   - Expected: 0 broken links (the bare wikilink stays `[[b]]` and
     resolves via stem lookup to `archive/b.md`).
   - Actual on Linux: 1 broken link, `target: "b"`, `path: null`.
   - Likely cause: `discovery::resolve_target`'s bare-stem fallback (or the
     case_index it consults) behaves differently on case-sensitive
     filesystems for this scenario. macOS APFS hides the bug.

## Why it slipped

`hyalo` is developed on macOS where APFS is case-insensitive by default.
Tests that rely on filesystem case behavior pass locally and on
`macos-latest`, so the developer feedback loop never surfaces these
regressions. CI on Ubuntu/Windows catches them, but the team has been
merging through.

## Suggested fix path

1. **Reproduce on Linux** — either via Docker (`rust:latest` image) or
   GitHub Actions on a branch with extra logging. The pattern strongly
   suggests it's in `link_fix::classify_fix` (test 1) and/or
   `discovery::resolve_target` + `case_index` interaction after a file
   move (test 2).

2. **Test 1 fix**: review the strategy enum priorities in `link_fix`.
   `LinkCaseMismatch` should probably take precedence over
   `ShortFormStemMismatch` when both apply — the user's stated intent
   in writing `[[Iteration_Protocols]]` is "this same file with
   different casing", not "find this stem anywhere".

3. **Test 2 fix**: run the test with `RUST_LOG=debug` on Linux to trace
   `resolve_target` for `[[b]]` after the mv. Likely a case_index
   population issue specific to the bare-stem path after move.

4. **Add a Docker-based CI matrix or local cross-test script** so this
   class of bug surfaces during development, not only in CI.

## Acceptance criteria

- [ ] Both tests pass on `ubuntu-latest` and `windows-latest`
- [ ] Root cause documented (which classifier branch, which case path)
- [ ] Regression coverage: a unit test in `link_fix` or `discovery`
      that would have caught this without an e2e
- [ ] Notes on a sustainable cross-platform feedback loop for future
      filesystem-case bugs

## Why not now

This affects a release-blocker for a clean CI run. But debugging
filesystem-case behavior without a case-sensitive filesystem to hand
is speculative. Best done with a Linux environment available — Docker
image, dedicated branch with CI logging, or temporary cross-OS dogfood
machine. The bug has been latent at least since iter-136 merged, so a
short defer to do it properly is preferable to a guess-fix.

## References

- PR #157 CI: same failures, merged anyway
- PR #158 CI: same failures, merged anyway
- Iteration that introduced the regressions: [[iterations/done/iteration-136-wikilink-md-suffix-and-short-form-mv]]
