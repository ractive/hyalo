---
type: iteration
title: "Iteration 269 — mv: scan beyond the backlinks graph for split frontmatter links"
date: 2026-09-04
status: planned
tags:
  - iteration
  - mv
  - links
  - dogfooding
branch: iter-269/mv-frontmatter-link-scan-gap
priority: 9
related:
  - "[[backlog/mv-frontmatter-split-link-detection-gap]]"
  - "[[iterations/iteration-262-frontmatter-wikilinks-first-class]]"
---

# Iteration 269 — mv: scan beyond the backlinks graph for split frontmatter links

## Goal

Iteration 262 (FM-2) taught `hyalo mv` to warn when a frontmatter `[[…]]` spans a line
break — a folded (`>`) or literal (`|`) block scalar, or a wrapped quoted string —
instead of silently leaving it dangling. Its PR #305 review (2026-09-04) found the
warning only fires for a file `mv` already scans for another reason: `plan_mv`'s file
set (`by_source`, in `crates/hyalo-core/src/link_rewrite.rs`) comes from
`LinkGraph::backlinks_ci(old_rel)`, and the FM-1 graph scanner
(`extract_frontmatter_links`) never extracts a split-across-lines wikilink as a graph
edge in the first place. So a file whose *only* reference to the moved target is the
folded/wrapped link gets neither a rewrite nor a warning — exactly the
silent-dangling-reference failure mode FM-2 exists to close, for precisely the case
where the split link is the file's only connection to the target. Full repro and
analysis in [[backlog/mv-frontmatter-split-link-detection-gap]].

The same probing found a related, pre-existing gap in `NEW-3`'s ambiguous-bare-link
detection (`skipped_ambiguous`), present on `main` before iteration 262 and not
introduced by it: two files sharing a stem where *neither* sits at the vault root are
never flagged as ambiguous by `mv`, and which of two same-stemmed candidates gets
flagged depends on which one is being moved. Both gaps stem from the same root cause —
`mv` only ever looks at files the backlinks graph already flagged — so this iteration
folds them into one fix.

Constraint: **no new CLI flags** from dogfood pressure (project rule). This is a
detection-completeness fix inside `plan_mv`, not a new flag or config key. Out of
scope: any change to what counts as a graph edge for `backlinks`/`summary`/`--orphan`
(FM-1's narrower scope — a split-across-lines link is deliberately not counted there,
and this iteration must not change that).

## Tasks

### SCAN-1: widen `plan_mv`'s file set for split-link detection

- [ ] Decide the mechanism: either (a) `plan_mv` runs a cheap secondary pass over every
      vault file not already in `by_source`, gated on the frontmatter block containing an
      unclosed `[[` (skip immediately if it doesn't, so files with no candidate line pay
      almost nothing), reusing `split_frontmatter_wikilink`'s existing whitespace-collapsed
      substring test; or (b) `LinkGraph`/`FileLinks` gains a lightweight "unresolved split
      occurrence" side-channel populated during the normal build pass (which already reads
      every file), consulted only by `plan_mv`. Record the choice as a DEC in
      [[decision-log]] — (a) is simpler and scoped to `mv`; (b) reuses work the build pass
      already does but couples an `mv`-only concern into the shared graph.
- [ ] Implement for single-file `mv`. Decide whether batch `mv` gets the same treatment in
      this iteration or a follow-up — batch already accepts a higher per-call cost, but the
      widened scan multiplies by every move in the batch.
- [ ] Fold `NEW-3`'s ambiguous-bare-link detection into the same widened scan (or record why
      not, if the mechanisms turn out not to share the necessary plumbing).
- [ ] Unit/e2e tests: a vault where a file's *only* reference to the moved target is a
      split frontmatter wikilink — `mv` reports it under `frontmatter_links_skipped` (JSON)
      and prints the stderr warning (text) with no other link to the target anywhere else in
      the vault. A second fixture for the `NEW-3` case: two same-stemmed files, both nested
      (no candidate at the vault root), a bare-link reference to the ambiguous stem, and `mv`
      of either candidate reports `skipped_ambiguous` regardless of which one moves.
- [ ] Confirm unchanged: `backlinks`/`summary`/`--orphan` still do not treat a split
      frontmatter link as a graph edge.

### SCAN-2: perf check

- [ ] Measure `mv` on Obsidian Hub (6520 files, `../obsidian-hub`) with no split links
      present, before and after — must stay within noise of the iter-262 baseline
      (`summary` was 0.40 s median of 3; `mv` has no existing baseline, so establish one).
- [ ] If the widened scan shows up in the measurement, consider limiting it to files whose
      frontmatter is at least scanned already for other reasons, or add a size/count guard
      with a documented tradeoff — do not add a new CLI flag to opt out (project rule).

## Acceptance criteria

- [ ] Reproduces then fixes the exact repro in
      [[backlog/mv-frontmatter-split-link-detection-gap]]: a file whose only reference to
      the moved target is a line-spanning frontmatter wikilink gets the warning.
- [ ] The related `NEW-3` gap (two nested same-stemmed files) is fixed or explicitly
      deferred with a recorded reason.
- [ ] `backlinks`/`summary`/`--orphan`/`find --dead-end` behavior is byte-for-byte
      unchanged on both `../kepano-obsidian` and the own knowledgebase.
- [ ] Perf: `mv` on `../obsidian-hub` with no split links stays within noise of a
      newly-established baseline.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, xtask help-drift check.
- [ ] Changelog entry via `hyalo changelog add`; the DEC from SCAN-1 recorded in
      [[decision-log]].

## Links

- [[backlog/mv-frontmatter-split-link-detection-gap]]
- [[iterations/iteration-262-frontmatter-wikilinks-first-class]]
- [[decision-log]]
