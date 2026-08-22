---
title: Iteration 206 — profile the links command's 12.7s GitHub Docs run
type: iteration
date: 2026-08-22
status: planned
branch: iter-206/links-perf-profiling
tags:
  - iteration
  - links
  - performance
related:
  - "[[dogfood-results/dogfood-v0210-pre-release-iters-191-198]]"
  - "[[iteration-200-links-apply-integrity]]"
  - "[[iteration-203-index-md-resolution]]"
---

# Iteration 206 — profile the links command's 12.7s GitHub Docs run

## Goal

Profile and address the `links` command's ~12.7s runtime observed against
the GitHub Docs corpus during the v0.21.0-pre dogfood. This was flagged as
a non-goal / "profile separately" item in both
[[iteration-200-links-apply-integrity]] and
[[iteration-203-index-md-resolution]] and never got its own plan — this
iteration is that plan.

## Context

Carried over from iteration 200's Non-goals section:

> The `links` 12.7s perf issue on GitHub Docs — profile separately.

No repro numbers or profiling data were captured beyond the headline
figure at dogfood time. First task here is therefore to reproduce and
measure before proposing a fix — do not assume the cause.

Corroborating data point from iter-203's MDN measurement run (14,375
files, read-only): `links` still took ~80s there too, unchanged by
iter-203's directory-index resolution — so the cost is orthogonal to
that change and reproduces on a second, larger corpus.

## Tasks

- [ ] Reproduce: run `hyalo links fix` (dry-run) and `hyalo links auto`
      (dry-run) against a GitHub Docs scratch copy of comparable size to
      the iter-200 corpus (961 files: `actions`, `graphql`, `get-started`,
      `code-security`) and record wall-clock time for each subcommand
      separately — the 12.7s figure did not distinguish them.
- [ ] Profile the slower of the two (perf/flamegraph or `cargo flamegraph`
      per the project's existing perf tooling) to identify the hot path —
      likely candidates given iter-200's changes: `LinkMatcher`
      construction (stem index build), the Jaro-Winkler fuzzy pass across
      all broken links, or repeated full-vault scans per source file.
- [ ] Determine whether iteration 200's changes (site-prefix stripping in
      `find_match`, the round-trip guard's extra normalization pass per
      fix) measurably moved this number, up or down, and record before/
      after if `git stash`-comparable.
- [ ] Fix or document: if a straightforward algorithmic fix exists (e.g.
      avoid O(n*m) fuzzy comparison, cache a normalization), implement and
      re-measure. If the fix is structural (e.g. needs the index/mmap
      work from earlier perf iterations), record findings and defer to a
      follow-up with a named approach instead of a vague "profile more."
- [ ] Add a regression guard appropriate to the finding — either a perf
      benchmark threshold (if the project has one for `links`) or a
      dogfood note to re-check next release.

## Acceptance criteria

- [ ] `links fix` and `links auto` dry-run timings on the GitHub Docs
      scratch copy are measured and recorded in this file, separately
      from each other
- [ ] The dominant cost is identified with profiling data, not guesswork
- [ ] Either a measured improvement lands, or the root cause and a
      concrete follow-up plan are documented here with enough detail that
      a future iteration can act without re-profiling from scratch
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Rearchitecting the index/storage layer wholesale — if profiling points
  there, scope a narrow follow-up rather than doing it here.
