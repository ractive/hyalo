---
title: Iteration 190 — link anchors (L-21) as one unit
type: iteration
date: 2026-07-19
status: completed
branch: iter-190/link-anchors
tags:
  - iteration
  - links
  - semantics
  - anchors
depends-on: "[[iterations/iteration-189-resolver-classify-collapse]]"
related:
  - "[[reviews/link-handling-review-2026-07-18]]"
  - "[[iterations/iteration-188-link-semantics-completion]]"
  - "[[iterations/iteration-185-link-semantics]]"
---

# Iteration 190 — link anchors (L-21) as one unit

## Goal

Land L-21 — anchors carried through resolution and validated — as the ONE
atomic unit that iter-185 and iter-188 both deferred (DEC-058,
iter-188 task 3): the `Link.fragment` index wire-shape bump, the
exact-heading anchor matcher, the distinct broken-anchor category in
`find --broken-links`, the index-path perf guard, and the anchor e2e
corpus. Shipping any subset would be a half-done shape change — worse than
the deferral. Builds on iter-189's collapsed resolver so anchor validation
hangs off the single shared entry point, not a new ad-hoc loop.

**Do NOT release; release is a separate user-gated step.**

**Lessons carried from iter-189 (apply here):**

- **Re-derive `discovery.rs` line citations before use.** iter-189 inserted
  ~370 lines into discovery.rs (the Classify-mode collapse: `LinkResolution`,
  `StemIndex`, `classify_link`, `classify_short_form_wikilink`,
  `classify_link_from_source`, `normalize_link_target` all now live there,
  above `percent_decode_path` / `resolve_target`). Every `discovery.rs:NNN`
  citation in this plan that predates iter-189 is a first-pass estimate —
  confirm against `main` at the branch point before relying on it, the same
  way iter-189 had to re-derive its own citations against post-187/188 main.
- **Anchor validation should build on `classify_link_from_source` /
  `resolve_link_from_source`, not a third loop.** iter-189's collapse means
  there is now exactly one Exists-mode and one Classify-mode entry point in
  discovery.rs; task 3's `find --broken-links` wiring should call the
  existing `resolve_link_from_source` for the target and add the anchor
  check as a second, independent step on its `Some(path)` result — do not
  duplicate the kind-dependent target-normalization branching that
  `normalize_link_target` already owns.
- **AC-fidelity gate wants the load-bearing symbol on the same line as its
  `- [ ]`.** `ac-fidelity-check.sh` only regex-matches the checkbox's first
  line; continuation lines are invisible to it. When ticking task/AC boxes
  in this plan, put the backtick-quoted test name or symbol that proves the
  claim on the `- [x]` line itself, not wrapped onto a later line.
- **A single squashed PR commit means "commit history proves before/after"
  phrasing doesn't hold.** If task 1/6's e2e-locks-before-migration
  sequencing doesn't survive as separate commits into the PR, phrase the AC
  as "e2e present and green in the shipped tree" rather than relying on
  commit ordering as the evidence.

**Constraints inherited (iter-188 task 3 + DEC-058/059):**

- ONE wire-shape change: `fragment: Option<String>` on `Link`
  (links.rs:51-58) — serialized into `.hyalo-index` via
  `IndexEntry.links: Vec<(usize, Link)>` (index.rs:49) AND the persisted
  `LinkGraph` (`BacklinkEntry.link`, link_graph.rs:15-22). Old snapshots
  must fail safe to disk scan with the existing warning; CHANGELOG notes
  the rebuild recommendation.
- iter-184 bucket lesson: broken anchors are a separately-reported
  category and must NOT inflate the headline `broken`/`fixable` counts or
  the "Apply N fixes" hint (commands/links.rs:246-253).
- Perf claims need a real measurement before ticking.
- Verified at plan time (do not "fix" what isn't broken): `mv` already
  preserves fragments on rewrite — body links via span splice + fragment
  re-append (`split_target_fragment` link_rewrite.rs:314-323, outbound
  :851-877, batch :1380-1397) and frontmatter via the L-2/L-7
  fragment+alias-aware rewrite (link_rewrite.rs:348-372), with unit tests
  (link_rewrite.rs:1550, :1564, :1658, :2099). Task 6 locks this with
  CLI-level e2e; it is not expected to change.

## Tasks

### 1. `Link.fragment` — the one wire-shape change [6/6]

- [x] Added `fragment: Option<String>` to `Link` (links.rs) with `#[serde(default, skip_serializing_if)]`; every `Link { .. }` literal updated (compiler-driven, workspace builds clean)
- [x] Parse side captures the fragment WITHOUT `#` via new `split_target_and_fragment`; fragment-only guards unchanged — pinned by `parse_wikilink_fragment_and_alias`, `parse_markdown_link_fragment`, `wikilink_only_fragment`, `span_fragment_only_skipped`
- [x] Rewrite-span invariant asserted: `span_wikilink_with_fragment` and `span_markdown_fragment_roundtrips_and_span_untouched` prove `target_end` stops before `#` and `link.fragment` round-trips while the `#…` bytes stay untouched
- [x] Old-snapshot behavior verified empirically (probe, both named+array framing): under `to_vec_named` the additive field is backward-compatible, old snapshots decode with `fragment: None` (fail-safe) — recorded as a DEC-060 deviation from the plan's hard-break premise; e2e `rebuilt_index_repopulates_fragments` locks the index path
- [x] CHANGELOG updated: wire-shape change note + `create-index` rebuild recommendation
- [x] DEC-060 records markdown-fragment percent-decoding (decode via `percent_decode_path` for matching only; written form preserved); `resolve_target` still strips fragment/query itself so target resolution is unchanged

### 2. Exact-heading anchor matcher (new shared helper) [4/4]

- [x] NEW helper `hyalo-core/src/anchor.rs::fragment_matches_headings`, explicitly separate from `SectionFilter` (exact existence check, not the `--section` substring selector)
- [x] Obsidian convention recorded in **DEC-060**: match iff trimmed heading == percent-decoded+trimmed fragment, ASCII case-insensitive — proven by `exact_match`, `case_insensitive_match`, `percent_encoded_case_insensitive`
- [x] `^block-id` refs skipped via `anchor::is_block_ref` — proven by `block_ref_always_valid`
- [x] Unit tests cover exact/trim/case/multiple/no-match/`heading:None`/unicode/percent-encoded/`^block`: `trim_heading_and_fragment`, `multiple_headings_one_matches`, `no_match_reports_false`, `heading_none_never_matches`, `unicode_heading`, `percent_encoded_fragment`, `empty_fragment_is_valid`, `literal_percent_not_decoded` (12 tests, all green)

### 3. Distinct broken-anchor category in `find --broken-links` [5/5]

- [x] `LinkInfo` gained `fragment: Option<String>` + `broken_anchor: bool`, both `#[serde(skip_serializing_if)]`; text output via unified `LINK_INFO_ANCHORED_FILTER` (all 6 fragment-bearing key signatures mapped) — covered by `link_info_anchored_resolved_filter`, `link_info_anchored_broken_anchor_filter`, `link_info_anchored_broken_target_filter`, `link_info_anchored_with_label_filter`
- [x] Wired into find after `resolve_link_from_source`: anchor check runs only on `Some(path)` with a non-`^` fragment; `path: None` is broken-TARGET only — proven by e2e `anchor_matrix_disk_scan` / `anchor_matrix_indexed` asserting `broken_target.md` is never also `broken_anchor`
- [x] `--broken-links` filter includes broken-anchor files (`l.path.is_none() || l.broken_anchor`); JSON distinguishes via `path` vs `broken_anchor` — e2e matrix
- [x] `links fix` headline counts NOT inflated: `detect_broken_links_from_index` still uses `classify_link_from_source` on `link.target` only — e2e `links_fix_ignores_broken_anchors` asserts `broken: 0, fixable: 0` for a `[[Foo#nope]]`-only vault
- [x] `backlinks`/graph unchanged (keys fragment-free) — e2e `backlinks_finds_anchored_linkers` proves `backlinks Foo.md` still finds the `[[Foo#Real]]` / `[[Foo#nope]]` linkers

### 4. Perf guard [4/4]

- [x] With `--index`: anchors validated against `index.get(target).sections` (`IndexEntry.sections`) — ZERO file reads on the index path (headings already indexed)
- [x] Without index: ScannedIndex already materializes `sections` in-memory when `--broken-links` scans bodies, so anchor validation is an in-memory heading lookup — no extra per-target re-read (strictly better than the planned memoized-HashMap approach)
- [x] A/B timing recorded in the table below (hyperfine `-N -w 3 -m 12`, KB vault): both disk and index paths within noise
- [x] AC met: indexed 29.7→30.2 ms (1.02×, within σ), disk 30.3→30.1 ms (1.00×) — zero I/O added on either path

Method: `hyperfine -N -w 3 -m 12`, release binaries, `hyalo-knowledgebase`
vault, `find --broken-links --fields links --format json`. `main` = origin/main
(d1e8b4f, iter-189 head) worktree binary; `branch` = iter-190 HEAD.

| bench | pre-anchor (iter-189 head) | branch | delta |
| --- | --- | --- | --- |
| find-broken-links (disk) | 30.3 ms ± 2.8 | 30.1 ms ± 1.7 | 1.00× (within noise) |
| find-broken-links (--index) | 29.7 ms ± 1.8 | 30.2 ms ± 1.7 | 1.02× (within σ) |

Both within measurement noise. The anchor branch adds no extra file reads:
target headings come from the already-materialized `IndexEntry.sections`
(index path) / already-scanned sections (disk path), so anchor validation is an
in-memory heading-set lookup per anchored link — zero I/O on either path.

### 5. HYALO006 interaction — decide and record [2/2]

- [x] Decided: HYALO006 stays TARGET-only this iteration — anchors surface only in `find --broken-links` (DEC-061), soaking one release before any lint/CI gate; own-rule-vs-sub-severity inclusion is an explicit follow-up
- [x] **DEC-061** recorded (excluded, with rationale); HYALO006 description (`engine.rs:170`) and README lint section now state anchors are not checked by the rule

### 6. mv fragment preservation — verify and lock [2/2]

- [x] Re-verified on the branch: mv does NOT drop fragments; the span splice invariant (target_end before `#`) holds after task 1's parse changes — no regression, no fix needed
- [x] CLI-level e2e added: `mv_preserves_fragments_dry_run`, `mv_preserves_fragments_apply` (wikilink + markdown + frontmatter `"Foo#Real"` all rewrite to Bar with `#Real` intact), and `batch_mv_preserves_fragments` (batch apply preserves `#Real` on the rewritten markdown path)

### 7. e2e matrix [8/8]

All in `crates/hyalo-cli/tests/e2e/anchors.rs` (9 tests, all green):

- [x] `[[Foo#Real]]` (Foo has `## Real`) → not broken: `assert_matrix` asserts `valid_anchor.md` not surfaced
- [x] `[[Foo#nope]]` → broken-ANCHOR (target resolves, `broken_anchor: true`, `path: Foo.md`): `assert_matrix`
- [x] `[[Nope#x]]` → broken-TARGET only, no anchor finding on the same link: `assert_matrix` asserts `broken_target.md` never `broken_anchor`
- [x] `[[Foo#^block]]` → skipped/valid, never reported: `assert_matrix` (`block_ref.md` not surfaced)
- [x] Markdown fragment variants `[t](Foo.md#Real)` + percent-encoded `[t](Foo.md#my%20heading)` resolve target AND anchor; `#missing` is broken: `assert_matrix` md_variants block (angle-form parsing unchanged; covered by existing L-A1 span tests)
- [x] Fragment-only `[[#h]]` / `[t](#h)` remain non-links: `assert_matrix` asserts `same_file.md` not surfaced
- [x] Index parity: `anchor_matrix_indexed` runs the same `assert_matrix` under `--index`; `rebuilt_index_repopulates_fragments` covers the rebuild path (see task 1 for the backward-compat/graceful-degradation deviation)
- [x] mv preservation e2e (task 6) green in the same run: `mv_preserves_fragments_*`, `batch_mv_preserves_fragments`

### 8. Docs, decisions & retrospective [4/4]

- [x] DEC-060 (anchor-match convention + fragment percent-decoding + shape backward-compat deviation) and DEC-061 (HYALO006 stays target-only) recorded in [[decision-log]]
- [x] README `find --broken-links` anchor section added; CHANGELOG shape-bump + rebuild note + new category; `templates/rule-knowledgebase.md` (symlinked) updated with the anchor row and HYALO006 target-only clarification — no release
- [x] Deferred annotations flipped: iter-188 task 3 header → resolved-in-190; review L-21 disposition → RESOLVED
- [x] KB edits via `hyalo set`/`read`/`lint` per CLAUDE.md

## Acceptance Criteria

- [x] ONE `Link` wire-shape change landed with parse-side capture, matcher (`anchor.rs`), distinct broken-anchor category, perf guard, and CHANGELOG rebuild note — no partial subset
- [x] Old `.hyalo-index` snapshots handled fail-safe (DEC-060 deviation): under `to_vec_named` framing they decode with `fragment: None` (no false anchor reports), not a hard disk-scan fallback — verified empirically; `rebuilt_index_repopulates_fragments` locks the index path
- [x] `find --broken-links` distinguishes broken-target vs broken-anchor, never both on one link; JSON (`anchor_matrix_*`) and text (`link_info_anchored_*_filter`) covered
- [x] `links fix` headline `broken`/`fixable`/"Apply N fixes" unchanged by broken anchors — `links_fix_ignores_broken_anchors`
- [x] Rewrite spans untouched: mv/links-fix preserve fragments byte-exact — unit `span_markdown_fragment_roundtrips_and_span_untouched` + CLI `mv_preserves_fragments_apply`
- [x] Indexed anchor validation performs zero file reads via `index.get(target_path).sections` / `fragment_matches_headings` (see `find/mod.rs` broken-anchor check); unindexed uses already-scanned in-memory sections; A/B numbers recorded in the task-4 table (hyperfine, not diff-verifiable — see table above)
- [x] DEC-060 and DEC-061 recorded; README/CHANGELOG/rule-template/HYALO006-desc in sync
- [x] `cargo fmt` / `clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace -q` all clean
