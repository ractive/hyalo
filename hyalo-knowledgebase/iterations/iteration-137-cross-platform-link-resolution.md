---
title: >-
  Iteration 137 — Fix cross-platform link-resolution test failures
type: iteration
date: 2026-05-23
status: planned
branch: iter-137/cross-platform-link-resolution
tags:
  - iteration
  - bug-fix
  - links
  - cross-platform
  - testing
related:
  - "[[backlog/cross-platform-link-resolution-failures]]"
  - "[[iterations/done/iteration-134-links-fix-short-form-wikilinks]]"
  - "[[iterations/done/iteration-136-wikilink-md-suffix-and-short-form-mv]]"
---

## Goal

Restore green CI on `ubuntu-latest` and `windows-latest`. Two link-resolution
e2e tests have been failing on case-sensitive filesystems since iter-136 and
have shipped through PR #157, #158, #159, and #160. macOS APFS is
case-insensitive by default and hides the bug; CI is the only signal.

A clean release for 0.17 (and credibility for any release that ships with
cross-platform claims) needs this resolved.

## Failing tests

1. **`links::case_insensitive_links_fix_dry_run_reports_case_mismatches`**
   - On-disk: `iteration_protocols.md` (lowercase). Wikilink:
     `[[Iteration_Protocols]]`. `.hyalo.toml` sets
     `[links] case_insensitive = "true"`.
   - Expected strategy: `"LinkCaseMismatch"`
   - Observed on Linux: `"ShortFormStemMismatch"`
   - Hypothesis: the short-form-stem detection added in iter-136 takes
     precedence over case-mismatch classification. Strategy enum priorities
     need review.

2. **`mv::mv_bare_wikilink_no_broken_links_after_move`**
   - `a.md` contains `[[b]]`. `mv b.md → archive/b.md`. Expected: 0 broken
     links after the move (the bare wikilink stays `[[b]]` and resolves via
     stem lookup to `archive/b.md`).
   - Observed on Linux: 1 broken link — `target: "b"`, `path: null`.
   - Hypothesis: `discovery::resolve_target`'s bare-stem fallback, or the
     `case_index` it consults, behaves differently on case-sensitive
     filesystems after a file move. APFS hides the bug because
     `is_file("tmp/b.md")` may still succeed via case-insensitive matching
     even though the file was moved.

## Steps

- [ ] Reproduce both failures in a Linux environment — either:
  - Docker `rust:latest` from the repo root, mount `target/`, run
    `cargo test --workspace`
  - or a feature branch with `RUST_LOG=debug` added to the failing tests
    and pushed to surface logs in CI
- [ ] Trace the first test: where does `LinkCaseMismatch` lose to
  `ShortFormStemMismatch` in the strategy enum? Pick a winning strategy
  that reflects user intent — a wikilink that's an exact case-variant of
  an existing file should be case-mismatch, not stem-mismatch.
- [ ] Trace the second test: does `case_index` see `archive/b.md` when
  `find --broken-links` runs after the `mv`? Does `discovery::resolve_target`
  hit the stem-lookup branch for bare `[[b]]`? On case-sensitive FS, what
  diverges?
- [ ] Fix the underlying bug(s). Prefer narrow fixes over reshuffling the
  classifier.
- [ ] Add regression coverage at the unit-test level (in `link_fix` or
  `discovery`) so future case-sensitivity regressions surface without
  needing a full e2e + foreign-OS round-trip.
- [ ] Confirm both tests pass on `ubuntu-latest` + `windows-latest` in CI
  on the iteration branch before merge.

## Tasks

- [ ] Repro on Linux (Docker or CI branch with debug logging)
- [ ] Diagnose test 1 (case-mismatch vs stem-mismatch precedence)
- [ ] Diagnose test 2 (bare-stem resolution after mv)
- [ ] Land fixes
- [ ] Add unit-level regression tests
- [ ] Verify Linux + Windows CI green
- [ ] Move `backlog/cross-platform-link-resolution-failures.md` →
  `backlog/done/`
- [ ] Update CHANGELOG

## Acceptance criteria

- [ ] `cargo test --workspace` is green on `ubuntu-latest`,
  `macos-latest`, and `windows-latest`
- [ ] Root-cause documented in commit messages (which classifier branch /
  which case_index path)
- [ ] Unit-level regression coverage exists for both bugs
- [ ] Backlog item closed

## Why now

The cross-platform breakage has been latent since iter-136 (≈ early May
2026). Each subsequent PR has shipped through it, normalising the red CI
signal. Three reasons to pay the debt now:

1. **Release credibility.** `gh release create` builds binaries for
   Linux + Windows + macOS package managers. Shipping with known
   cross-platform link-resolution bugs erodes user trust.
2. **Signal restoration.** While CI is red on these tests, it can't
   catch any new cross-platform regression — every future PR will
   inherit "Linux + Windows red".
3. **Debt compounds.** The longer the gap, the harder it is to bisect
   if a third cross-platform issue lands on top.

## Non-goals

- Setting up a permanent Docker-based local cross-test workflow. That's a
  separate (smaller) backlog item if the value materialises.
- Refactoring the link-fix strategy classifier beyond the minimum needed
  to pass the test correctly.
