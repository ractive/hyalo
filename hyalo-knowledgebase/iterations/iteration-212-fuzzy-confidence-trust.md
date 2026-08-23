---
title: Iteration 212 — fuzzy confidence trust (scoring, floor, honest labels)
type: iteration
date: 2026-08-23
status: planned
branch: iter-212/fuzzy-confidence-trust
tags:
  - iteration
  - links
related:
  - "[[dogfood-results/dogfood-v0210-pre2-integrity-wave]]"
  - "[[iterations/iteration-200-links-apply-integrity]]"
---

# Iteration 212 — fuzzy confidence trust (scoring, floor, honest labels)

## Goal

Make `--apply-fuzzy` trustworthy: confidence scores that track semantic
plausibility, a default floor so a bare `--apply-fuzzy` does not accept
garbage, and strategy names surfaced honestly in the text output.

## Context

From [[dogfood-results/dogfood-v0210-pre2-integrity-wave]] BUG-11
(MEDIUM). The current confidence is a normalized string distance over
full paths, which long slugs inflate. Real proposals from the GitHub
Docs corpus:

- `/actions/reference/actions-limits` →
  `graphql/reference/actions.md` — **0.9** (wrong document)
- `/billing/reference/actions-minute-multipliers` →
  `code-security/.../actions-built-in-queries.md` — **0.888** (wrong)
- `/code-security/how-tos/scan-code.../configuring-larger-runners-for-default-setup`
  → `code-security/how-tos/find-and-fix.../configuring-larger-runners-for-default-setup.md`
  — **0.6** (the only correct one, lowest score)

The ordering is inverted relative to usefulness. `fuzzy_min_confidence`
defaults to null, so a bare `--apply-fuzzy` on that corpus applies 1,047
rewrites at ≥0.8 to unrelated documents. Additionally the text renderer
labels every gated fix `[fuzzy N]` regardless of strategy, so the
honest `BasenameFallback` name introduced by iter-200 (M-1) never
reaches the user.

Prerequisite ordering: [[iterations/iteration-210-output-truth]] adds
per-fix JSON detail; landing 210 first makes this iteration's evaluation
measurable by script. Not a hard dependency.

## Tasks

- [ ] Rework fuzzy scoring to weight the final path segment (the
      basename/slug) far above shared prefixes — e.g. score the basename
      match and the directory match separately and combine with the
      basename dominant. The three dogfood examples above must reorder:
      the correct proposal scores highest.
- [ ] Introduce a default minimum confidence for `--apply-fuzzy`
      (tune against the GitHub Docs corpus so the
      correct-relocation class survives and cross-tree garbage does
      not), overridable via `--min-confidence <0..1>` and
      `fuzzy_min_confidence` in `.hyalo.toml`. Proposals below the floor
      report as unfixable-with-candidate, not applied.
- [ ] Surface the strategy name in text output: `[fuzzy 0.87 basename]`,
      `[basename-fallback 0.6]`, etc. — whatever naming, `BasenameFallback`
      must be distinguishable from path-similarity fuzz.
- [ ] Tune and record evidence: on the GitHub Docs scratch copy, measure
      how many of the previous 1,047 ≥0.8 rewrites survive the new
      scoring+floor, and manually classify a sample (target ≥90%
      correct among applied). Record numbers and the chosen floor in
      this file and as a DEC.
- [ ] Docs/help: `links --help` documents the floor, the flag, the
      config key, and the strategy labels; CHANGELOG entry (behavior
      change for `--apply-fuzzy` users: fewer, better fixes).

## Acceptance criteria

- [ ] The three dogfood example proposals reorder so the correct one
      scores highest
- [ ] A bare `--apply-fuzzy` on the GitHub Docs copy applies a measured,
      documented, ≥90%-correct-in-sample set instead of 1,047
      indiscriminate rewrites; broken count still decreases
      monotonically
- [ ] `--min-confidence 0` restores the old accept-everything behavior
      (escape hatch), `--min-confidence 0.99` applies near-nothing
- [ ] Text output distinguishes `BasenameFallback` from path-similarity
      fuzzy fixes
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Machine-learned or embedding-based matching — string features only.
- Per-fix JSON plumbing — that is
  [[iterations/iteration-210-output-truth]].
