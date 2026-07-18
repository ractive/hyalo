---
title: "Iteration 178 — link surgical fixes (Phase A: anchors, self-links, stale graph)"
type: iteration
date: 2026-07-18
status: completed
branch: iter-178/link-anchor-integrity
tags:
  - iteration
  - links
  - mv
  - data-safety
related:
  - "[[reviews/link-handling-review-2026-07-18]]"
  - "[[dogfood-results/dogfood-v0180-final-pre-release]]"
---

# Iteration 178 — link surgical fixes (Phase A)

## Goal

Ship the surgical, independently-valuable link fixes from
[[reviews/link-handling-review-2026-07-18]] (Phase A) without waiting for
the scanner/resolver refactors. All root causes are confirmed with exact
file:line and repro fixtures; regression-test sketches live in
`.claude/agent-memory/rust-developer/pitfall_frontmatter_wikilink_fragment_loss.md`
and `pitfall_fuzzy_match_phantom_tie.md`.

Re-scoped 2026-07-18 after the 4-agent link deep review: the original
"likely" guesses are now confirmed, L-1 (worse than the dogfood finding)
is added, and the code-span + fuzzy-gate tasks moved to iterations 183
and 184 where their real fixes belong.

## Tasks

### 1. Shared frontmatter wikilink rewrite helper (L-1, L-2, L-7) [5/5]

- [x] Add a fragment- and alias-aware
  `rewrite_frontmatter_wikilink_text(occ, old_ref, new_ref) -> Option<String>`
  helper (use canonical `parse_wikilink` for matching; reattach
  `#fragment` and `|alias` from the original occurrence; reuse
  `split_target_fragment`, link_rewrite.rs:302-307)
- [x] L-2: `plan_frontmatter_wikilink_rewrites` (link_rewrite.rs:1179)
  matches anchored targets via the helper — `"[[decision-log#DEC-041]]"`
  in `related` is rewritten with the anchor preserved
- [x] L-7: `build_replacements_for_file` frontmatter block
  (link_fix.rs:997-1033) rebuilds via the same helper — repairs keep the
  anchor
- [x] L-1: `plan_outbound_rewrites` (link_rewrite.rs:630-826) and the
  batch counterpart (:1223-1325) invoke the frontmatter rewriter for the
  moved file itself — self-referencing frontmatter links survive a plain
  rename
- [x] Locking e2e matrix for BOTH mv and links fix: scalar/list/quoted
  frontmatter × plain/`#anchor`/`|alias`/`#anchor|alias` × inbound/self

### 2. One-liners and ordering fixes (L-5, L-8, L-9, L-14) [4/4]

- [x] L-5: mutation.rs:122 `refresh_entry` → `refresh_entry_and_links`;
  e2e: `mv --index` then `backlinks --index` shows the rewritten source
- [x] L-8: link_fix.rs:1050-1062 guards the `%%` comment-fence toggle
  with `!fence.in_fence()` (match auto_link.rs:527-536 ordering); test:
  literal `%%` inside a fenced block doesn't desync later rewrites
- [x] L-9: `LinkMatcher::find_match` (link_fix.rs:789-824) seeds
  best/second scores with `NEG_INFINITY` and gates on threshold after
  the loop; test: single candidate scoring within TIE_DELTA above
  threshold is accepted
- [x] L-14: single-file mv reuses batch mode's canonicalize-based
  same-path check (mv.rs:436-440) so `a.md` → `A.md` works on
  case-insensitive FS

### 3. Retrospective [2/2]

- [x] Verify the dogfood repro chain (mv → broken-link check →
  links fix) ends with zero broken links and zero lost anchors
- [x] Update iterations 183-185 with anything learned; changelog entries

## Acceptance Criteria

- [x] All Phase A findings (L-1, L-2, L-5, L-7, L-8, L-9, L-14) have locking tests reproducing the review's confirmed failures: L-1/L-2/L-7 via `rewrite_frontmatter_wikilink_text_preserves_fragment_and_alias`, `plan_mv_inbound_frontmatter_anchor_preserved`, `plan_mv_self_referencing_frontmatter_anchor_link_rewritten`, `plan_mv_batch_self_referencing_frontmatter_link_rewritten`, `build_replacements_frontmatter_repair_preserves_anchor_and_alias`; L-5 via `mv_index_refreshes_source_link_graph`; L-8 via `build_replacements_literal_percent_in_code_fence_does_not_desync`; L-9 via `matcher_single_candidate_inside_tie_delta_above_threshold_accepted` and `matcher_two_genuine_ties_still_rejected`; L-14 via `mv_case_only_rename_on_case_insensitive_fs`
- [x] No behavior change outside the fixed cases: `cargo test --workspace -q`
  is green (1294+847+53+29 tests pass, 0 failed) on top of the fixes above
- [x] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean
