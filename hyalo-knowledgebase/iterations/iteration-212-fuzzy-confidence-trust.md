---
title: Iteration 212 — fuzzy confidence trust (scoring, floor, honest labels)
type: iteration
date: 2026-08-23
status: completed
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

- [x] Rework fuzzy scoring to weight the final path segment (the
      basename/slug) far above shared prefixes — e.g. score the basename
      match and the directory match separately and combine with the
      basename dominant. The three dogfood examples above must reorder:
      the correct proposal scores highest.
- [x] Introduce a default minimum confidence for `--apply-fuzzy`
      (tune against the GitHub Docs corpus so the
      correct-relocation class survives and cross-tree garbage does
      not), overridable via `--min-confidence <0..1>` and
      `fuzzy_min_confidence` in `.hyalo.toml`. Proposals below the floor
      report as unfixable-with-candidate, not applied.
- [x] Surface the strategy name in text output: `[fuzzy 0.87 basename]`,
      `[basename-fallback 0.6]`, etc. — whatever naming, `BasenameFallback`
      must be distinguishable from path-similarity fuzz.
- [x] Tune and record evidence: on the GitHub Docs scratch copy, measure
      how many of the previous 1,047 ≥0.8 rewrites survive the new
      scoring+floor, and manually classify a sample (target ≥90%
      correct among applied). Record numbers and the chosen floor in
      this file and as a DEC.
- [x] Docs/help: `links --help` documents the floor, the flag, the
      config key, and the strategy labels; CHANGELOG entry (behavior
      change for `--apply-fuzzy` users: fewer, better fixes).

## Acceptance criteria

- [x] The three dogfood example proposals reorder so the correct one
      scores highest
- [x] A bare `--apply-fuzzy` on the GitHub Docs copy applies a measured,
      documented, ≥90%-correct-in-sample set instead of 1,047
      indiscriminate rewrites; broken count still decreases
      monotonically
- [x] `--min-confidence 0` restores the old accept-everything behavior
      (escape hatch), `--min-confidence 0.99` applies near-nothing
- [x] Text output distinguishes `BasenameFallback` from path-similarity
      fuzzy fixes
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Machine-learned or embedding-based matching — string features only.
- Per-fix JSON plumbing — that is
  [[iterations/iteration-210-output-truth]].

## Evidence

Corpus: GitHub Docs `~/devel/docs/content`, 3,710 files, copied to a scratch
directory. Ground truth: the `redirect_from:` frontmatter GitHub Docs maintains
on every page (9,467 old URLs indexed). A proposal `old → new` is *correct*
when `old` is one of `new`'s declared redirects, *wrong* when it is some other
file's declared redirect, *unknown* otherwise.

Baseline (`main` @ 095838f, v0.20.0 behaviour): 6,099 broken links, 4,659
low-confidence proposals, all of which a bare `--apply-fuzzy` writes.

| floor | applied | wrong | unknown | correct (of known) |
| --- | --- | --- | --- | --- |
| none (baseline) | 4,659 | 804 | 144 | 82.2% |
| 0.75 | 3,111 | 39 | 17 | 98.7% |
| **0.8 (chosen default)** | **2,253** | **15** | **3** | **99.3%** |
| 0.85 | 596 | 11 | 0 | 98.2% |
| 0.9 | 312 | 0 | 0 | 100% |

Accuracy by confidence band is monotone: `[0.90, 1.00)` 100%, `[0.80, 0.90)`
99.3%, `[0.70, 0.75)` 89.8%, `[0.40, 0.45)` 5.3%, below 0.30 zero. The score is
a usable signal, not decoration.

Monotonicity and idempotence: one `--apply --apply-fuzzy` pass takes broken
6,099 → 3,846; a second pass applies 0 more and the count holds at 3,846.

Escape hatches on the same corpus: `--min-confidence 0` would apply all 5,506
proposals, `--min-confidence 0.99` applies 0.

The three BUG-11 proposals, verbatim from the corpus:

| proposal | before | after |
| --- | --- | --- |
| `/actions/reference/actions-limits` → `graphql/reference/actions.md` (wrong) | 0.9 | 0.504 |
| `/billing/reference/actions-minute-multipliers` → `…/actions-built-in-queries.md` (wrong) | 0.889 | 0.533 |
| `…/scan-code-for-vulnerabilities/…/configuring-larger-runners-for-default-setup` → `…/find-and-fix-code-vulnerabilities/…` (correct) | 0.6 | 0.87 |

Rationale for 0.8 over 0.75, the alternative that also clears the ≥90% target,
is recorded in DEC-078.

## Outcome

- New module `crates/hyalo-core/src/link_score.rs` — soft-token basename
  similarity, prefix-dominant directory similarity, `candidate_confidence`,
  `DEFAULT_FUZZY_MIN_CONFIDENCE = 0.8`.
- `LinkMatcher::find_match` ranks fuzzy candidates by the composite score and
  gives `BasenameFallback` a computed confidence instead of the flat 0.6;
  `--threshold` keeps its Jaro-Winkler stem meaning as the candidacy gate.
- `FuzzyApply` gains a floor; `[links] fuzzy_min_confidence` in `.hyalo.toml`
  moves it without opting in to applying.
- Text output prints each proposal's own strategy (`[basename-fallback 0.87]`
  vs `[fuzzy-match 0.91]`), marks suppressed ones `— below floor`, and names
  the floor. JSON gains `fuzzy_below_floor` and per-fix `rule`/`below_floor`;
  `fuzzy_min_confidence` is now always the effective number.
- `hyalo config` reports `links.fuzzy_min_confidence`.
