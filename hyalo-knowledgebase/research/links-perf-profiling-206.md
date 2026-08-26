---
title: "Links perf profiling 206 — corpus generator + raw numbers"
type: research
date: 2026-08-26
status: active
tags:
  - links
  - performance
  - profiling
related:
  - "[[iteration-206-links-perf-profiling]]"
  - "[[dogfood-results/dogfood-v0210-pre2-integrity-wave]]"
---

# Links perf profiling 206 — corpus generator + raw numbers

Scratch corpora for [[iteration-206-links-perf-profiling]] live in
`/tmp/hyalo-githubdocs` (adversarial: every broken target unique) and
`/tmp/hyalo-githubdocs-real` (realistic: ~120 broken site-absolute targets
reused across 960 pages). Both are regenerable from the Python snippet in the
iteration file's git history (commit that landed iter-206) — 961/960 files,
four GitHub-Docs sections, 12–14 links per file.

## Raw phase timings (release, realistic corpus)

Baseline (`main` @ 380314d):

- `links fix` wall clock: **4.84 s**
- `links auto` wall clock: 0.05 s

After the iter-206 shortlist cache:

- `links fix` wall clock: **0.95 s**
- `links auto` wall clock: 0.05 s
- `links-perf` example: discover 8.4 ms → index 52 ms → detect 63 ms →
  matcher 0.6 ms → plan_fixes **661 ms** (was 4.16 s on the adversarial
  corpus).

## Profiling method

macOS `sample` (1 ms interval) against the debug-symbols build of
`cargo run -p hyalo-core --example links-perf <dir>` — release binaries ship
stripped, so symbolized attribution needs the dev profile even though it is
~16× slower. Hot leaf: ~87% of samples in `strsim::jaro_winkler` under
`LinkMatcher::find_match` (fuzzy-candidacy gate), ~7% in
`link_score::soft_token_f1` (confidence ranking) — the latter is what
dominates *after* the fix on the adversarial corpus.

## A/B binaries

- baseline: built from a `git worktree` at `main`
- candidate: `iter-206/links-perf-profiling` release build

## Follow-up candidates (deferred)

- `candidate_confidence` over the shortlist is now the ceiling on the
  adversarial all-unique-target case (2.5 s of 3.1 s). A bounded top-2
  heap keyed on a cheap pre-score would cut it further; not worth the
  complexity until a real corpus shows it matters.
- Rayon-parallelizing `plan_fixes` across broken links is now trivially
  safe (matcher is `&self`, cache via per-thread shortlists) if the gate
  ever needs it.
