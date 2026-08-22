---
title: Iteration 208 — links output ordering and hint/message UX residue
type: iteration
date: 2026-08-22
status: planned
branch: iter-208/links-output-and-hint-ux
tags:
  - iteration
  - links
  - cli
  - ux
related:
  - "[[dogfood-results/dogfood-v0210-pre-release-iters-191-198]]"
  - "[[iteration-206-links-perf-profiling]]"
  - "[[iteration-204-dogfood-low-batch]]"
---

# Iteration 208 — links output ordering and hint/message UX residue

## Goal

Carry-over from iteration 204's carry-over sweep: close the v0.21.0-pre
dogfood's UX-3 (output-ordering half only — the perf half is
[[iteration-206-links-perf-profiling]]) and the UX-5 sub-items that no
prior iteration (200-204, 206) picked up. All are text-output/message
polish, no behavior change.

**Do NOT release; release is a separate user-gated step.**

## Context

Repros in [[dogfood-results/dogfood-v0210-pre-release-iters-191-198]]
(UX-3, UX-5). Iteration 204's carry-over sweep (2026-08-22) confirmed
these were still unaddressed after iterations 200-204, 206-207 landed:

- UX-3 had two halves. The perf half ("`links` 12.7s on GitHub Docs") got
  its own plan in iter-206. The **output-ordering half did not**: `links`
  text output prints the bucket summary first, then thousands of
  unlabeled fix lines; `out_of_vault_links` / `unfixable_links` are
  JSON-only (invisible in the default text format); "Case mismatches: 17"
  scrolls off-screen before the rewrites it announces are shown.
- UX-5, checked sub-item by sub-item against 200-204/206-207:
  - "the `--dir` redundancy note actively misleads (see H-4)" — **done**,
    iter-201 (H-4).
  - hints repeat a long `--index-file` path verbatim 4-5× in one hint
    listing — **not done**.
  - `config_excluded`'s "Excluded … : 1 titles" reads like a failure when
    the match count doesn't move (it counts candidate *titles*, not
    links) — **not done**.
  - `[types.note]` config error suggests `schema` but not
    `hyalo types set` — **not done**.

## Tasks

- [ ] UX-3: redesign `links` text-output ordering so the dangerous/actionable
      information (unfixable links, out-of-vault links, case mismatches)
      is not buried under thousands of unlabeled fix lines — likely:
      summary buckets last (or repeated at the end), or a `--summary-only`
      style default with `-v`/`--verbose` for the full per-link listing.
      Get eyes on real output shape (GitHub Docs / MDN scratch copies)
      before deciding the exact layout; record the decision as a DEC
      entry.
- [ ] UX-3: surface `out_of_vault_links` / `unfixable_links` in text
      output, not just JSON — these are exactly the "needs a human"
      buckets the text reader is least likely to notice today.
- [ ] UX-5: de-duplicate a hint listing that repeats the same long
      `--index-file <path>` across 4-5 hints — factor the path out (e.g.
      state it once, reference it) or shorten repeated occurrences.
- [ ] UX-5: reword `config_excluded`'s count line so "Excluded … : 1
      titles" cannot read as "1 link excluded" — name what is actually
      being counted (candidate titles, not matches/links).
- [ ] UX-5: `[types.note]` (or any per-type) config error should suggest
      `hyalo types set` alongside/instead of a bare `schema` mention, so
      the fix path matches what the user actually runs.
- [ ] e2e coverage for each UX fix: golden/snapshot-style assertions on
      the reworded text, not just "contains substring", since these are
      exactly the kind of doc/message-drift the batch iterations (196,
      204) exist to prevent.
- [ ] CHANGELOG entries (UX polish, not breaking).

## Acceptance criteria

- [ ] A GitHub Docs or MDN scratch-copy `links fix`/`links auto` text run
      shows out-of-vault and unfixable links without scrolling past
      thousands of ordinary fix lines first
- [ ] `out_of_vault_links` / `unfixable_links` counts appear in text
      output when non-empty
- [ ] The reworded hint/message strings are asserted by e2e tests, not
      eyeballed
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- The `links` perf profiling itself — [[iteration-206-links-perf-profiling]]
  owns that.
- Any change to `links` matching/exclusion semantics — untouched here,
  output formatting only.
- L-3 (chmod 444 rewrite silently succeeds; atomic writes break hard
  links) and L-8 (index files not byte-reproducible) stay accepted
  behavior per iteration 204's Non-goals — not in scope here either;
  revisit only if a user report arrives.
