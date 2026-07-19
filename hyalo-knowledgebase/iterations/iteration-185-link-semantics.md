---
title: "Iteration 185 — link semantics extensions (Phase D: anchors, lint rule, escapes)"
type: iteration
date: 2026-07-18
status: planned
branch: iter-185/link-semantics
tags: [iteration, links, lint, features]
depends-on: "[[iterations/iteration-184-link-resolver-writer-unification]]"
related:
  - "[[reviews/link-handling-review-2026-07-18]]"
---

# Iteration 185 — link semantics extensions (Phase D)

## Goal

With one scanner (iter-183) and one resolver/writer (iter-184) in place,
add the semantics the review showed are missing — each lands in exactly
one place now. Findings L-16, L-19, L-21, L-22, L-23 from
[[reviews/link-handling-review-2026-07-18]].

**Carried over from iter-184 (Phase C):** iter-184 shipped the L-6 root
fix (an O(1) lowercased companion map in `LinkGraph`, `backlinks_ci`) plus
L-10/L-12/L-24/L-26, but deferred the two large mechanical refactors so its
PR stayed reviewable. Fold these into Phase D before/with the semantics
work: (a) the single `resolve_link(ctx, link, mode)` entry point collapsing
the find/mod, `link_fix`, `backlinks`, and `summary` call sites onto
`LinkResolver`/`LinkGraph`; (b) `auto_link::apply_matches` onto
`execute_plans`/`RewritePlan` with the stronger TOCTOU guard; (c) L-11
per-file partial-failure envelope (applied/failed/skipped) + batch-mv
rollback-vs-report semantics; (d) L-25 dry-run/apply single-path parity.

**Lessons from iter-184 PR review (apply to this iteration too):**

- **`LinkGraph.lower_index` must stay incrementally maintained, not
  rebuilt.** iter-184's first cut had `rename_path`/`remove_source`/
  `insert_links` each call a full O(vault) `rebuild_lower_index()` per
  invocation; since batch-mv calls these once per file, this regressed
  batch-mv throughput ~38-44% vs main (measured on a synthetic 2000-file
  vault) before being fixed in review to update only the changed
  `lower_index` buckets. Any new `LinkGraph` key-set mutation added by
  the `resolve_link` unification (item (a) above) or by L-11's
  partial-failure envelope work must follow the same incremental
  pattern — do not reintroduce a bulk rebuild inside a per-file loop.
  See `lower_index_stays_consistent_across_incremental_mutations` in
  `link_graph.rs` for the regression-test pattern (compares incremental
  state against a from-scratch rebuild) — extend it if new mutation
  methods are added.
- **"Reported separately, excluded from apply" buckets must exclude
  their own count from the general bucket, not just add a new field.**
  iter-184's fuzzy-match tier (L-10) initially left `fixable`/`fixes`
  counting fuzzy matches *in addition to* the new `fuzzy`/`fuzzy_fixes`
  bucket, so the dry-run "Apply N fixes" hint promised fixes that plain
  `--apply` didn't write. When L-22's broken-anchor category (or any
  other new low-confidence/opt-in bucket) is added, make sure the
  headline counts (`broken`, `fixable`, hint text) reflect only what the
  *default* action set actually touches, and add an e2e assertion that
  running the suggested hint command produces the promised result.
- **Perf claims need an actual measurement, not an assumption.**
  iter-184's plan had a ticked "perf on MDN unchanged" sub-claim that
  turned out to be unverified (no MDN corpus was benchmarked in that
  PR). Item 1's "Perf guard: ... MDN-scale timing within budget" task
  below should be backed by a real before/after timing run (MDN corpus,
  or — if unavailable — a synthetic vault at comparable scale with
  numbers recorded in the plan) before being ticked, not marked done on
  the strength of an untested assumption.

## Tasks

### 1. Anchor validation (L-21)

- [ ] Anchors are carried through resolution (not discarded at parse,
  links.rs:488-490): `[[Foo#nonexistent-heading]]` is reportable as
  broken-anchor by `find --broken-links` (distinct category from
  broken-target, since heading checks read file content)
- [ ] Heading→anchor slugging matches the wiki/Obsidian convention used
  by `read --section`; decide case/whitespace normalization and record
  in the decision log
- [ ] Perf guard: anchor validation only reads target files that are
  actually linked with an anchor; MDN-scale timing within budget

### 2. Broken-link lint rule (L-22)

- [ ] New HYALO004 lint rule: broken wikilink/markdown link targets
  (and optionally broken anchors, severity-configurable) so link health
  can gate CI via `lint --strict`
- [ ] Vault-level cache so lint doesn't rebuild the link graph per file;
  respects `[lint] ignore` and `[okf]`/exempt semantics
- [ ] Docs: lint-rules list/show entries, README, knowledgebase

### 3. Escapes and normalization (L-16, L-19, L-23)

- [ ] L-16: `\[[not-a-link]]` is not extracted (backslash escape per
  CommonMark/Obsidian); rewiters leave it untouched
- [ ] L-19: `.md`-suffix normalization happens at `Link` construction
  (with an as-written field preserved); remove the manual re-strip at
  auto_link.rs:552-557 and audit consumers comparing targets across
  link kinds
- [ ] L-23: percent-decode markdown link targets in `resolve_target`
  (discovery.rs:714-842) so `my%20page.md` resolves; encoding kept
  as-written on rewrite

### 4. Retrospective

- [ ] Re-run the full link-review fixture corpus (multi-line spans,
  BOM, CRLF, anchors, aliases, escapes) across find/mv/fix/auto/lint —
  all consistent
- [ ] Close out [[reviews/link-handling-review-2026-07-18]]: mark each
  L-finding fixed/deferred with a pointer

## Acceptance Criteria

- [ ] `hyalo lint --strict` can gate broken links in CI
- [ ] Anchored-link health visible in `find --broken-links` output
- [ ] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean
