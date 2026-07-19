---
title: Iteration 190 — link anchors (L-21) as one unit
type: iteration
date: 2026-07-19
status: planned
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

### 1. `Link.fragment` — the one wire-shape change [0/6]

- [ ] Add `fragment: Option<String>` to `Link` (links.rs:51-58) with
  `#[serde(default)]`; update every `Link { .. }` literal
  (compiler-driven, workspace-wide)
- [ ] Parse side — stop discarding fragments: capture the fragment in
  `parse_wikilink` (the `strip_fragment` call at links.rs:520) and
  `parse_markdown_link` (links.rs:617), storing it WITHOUT the leading
  `#`; keep the fragment-only guards exactly as-is (links.rs:522-525 and
  :619-622 — `[[#h]]` / `[t](#h)` stay non-file-links; pinned by
  `wikilink_only_fragment` :1005, `span_fragment_only_skipped` :1138)
- [ ] Rewrite-span invariant: `LinkSpan.target_end` already stops before
  `#` (wikilink links.rs:392-396/:411, markdown :466/:475) — assert
  unchanged (existing span tests :1052-1138) so every rewrite splice keeps
  bytes from `#` onward untouched; add a span test asserting
  `link.fragment` round-trips alongside the untouched span
- [ ] Old-snapshot fallback: rmp-serde's array framing makes the added
  field a hard schema break regardless of `#[serde(default)]` — old
  snapshots must land in `load_inner`'s `Err` arm ("index file is
  incompatible (…); falling back to disk scan", index.rs:743-748). e2e: a
  pre-bump `.hyalo-index` fixture (generated with the main-branch binary
  or a committed byte fixture) triggers the warning and produces
  disk-scan-identical results; `#[serde(default)]` stays for
  JSON/map-encoded contexts
- [ ] CHANGELOG: wire-shape change + `create-index` rebuild
  recommendation
- [ ] Decide + record (part of DEC-060, task 2): markdown fragments may be
  percent-encoded (`[t](note.md#my%20heading)`) — decode via
  `percent_decode_path` (discovery.rs:858) for matching only; the written
  form is preserved (spans never cover the fragment). Note
  `resolve_target` continues to strip fragment/query itself
  (discovery.rs:929-936) so target resolution is unchanged by this task

### 2. Exact-heading anchor matcher (new shared helper) [0/4]

- [ ] NEW helper (e.g. `hyalo-core/src/anchor.rs`), explicitly NOT
  `SectionFilter` (crates/hyalo-core/src/heading.rs:66 — the
  `--section` matcher; substring match, case-insensitive by default,
  regex modes: the wrong contract for validation). The KB's earlier
  "SectionSelector" name refers to this type
- [ ] Define the Obsidian-style convention explicitly and record
  **DEC-060**: a fragment matches a heading iff the trimmed heading text
  equals the (percent-decoded, trimmed) fragment; case-INSENSITIVE
  comparison proposed (Obsidian resolves `[[Foo#tasks]]` against
  `## Tasks`) — if implementation research contradicts this, decide and
  record the actual choice in DEC-060 either way
- [ ] `#^block-id` refs (fragment starting with `^`) are SKIPPED from
  validation — never reported broken (we do not index block ids)
- [ ] Unit tests: exact match, trim, case policy, multiple headings, no
  match, `heading: None` sections (pre-heading outline entries,
  types.rs:104 is `Option<String>`), unicode headings, percent-encoded
  fragment, `^block` skip

### 3. Distinct broken-anchor category in `find --broken-links` [0/5]

- [ ] `LinkInfo` (hyalo-core types.rs:72-76) gains anchor fields —
  proposal: `fragment: Option<String>` plus
  `broken_anchor: bool` with `#[serde(skip_serializing_if = ...)]` so
  non-anchored links keep today's JSON shape; text output templates
  updated (output.rs:423-435; keep the exhaustive-combination doc test at
  output.rs:553 in sync)
- [ ] Wire into find: after `resolve_link_from_source`
  (find/mod.rs:735-742) yields `Some(path)` and the link carries a
  non-`^` fragment, validate via the task-2 matcher; `path: None` links
  are broken-TARGET only — the anchor check never runs, so broken-target
  and broken-anchor are never double-reported on one link (AC)
- [ ] `--broken-links` filter (find/mod.rs:840-851) includes files with
  broken anchors, but JSON/text clearly distinguishes the two categories
- [ ] `links fix` headline counts NOT inflated (iter-184 bucket lesson):
  the detect path (`detect_broken_links_from_index` → iter-189's
  `classify_link_from_source`) continues to classify targets only —
  e2e: a vault whose only defect is `[[Foo#nope]]` reports
  `broken: 0, fixable: 0` and no "Apply N fixes" hint
  (commands/links.rs:246-253)
- [ ] `backlinks`/graph behavior unchanged: `insert_file_links` keys stay
  fragment-free (`Link.target` never contained the fragment); assert with
  an e2e that `backlinks Foo` still finds `[[Foo#head]]` linkers

### 4. Perf guard [0/4]

- [ ] With `--index`: validate anchors against `IndexEntry.sections`
  (index.rs:45, `OutlineSection.heading` at types.rs:102-110) — ZERO file
  reads on the index path (headings are already indexed)
- [ ] Without index: read ONLY the targets of anchored links, once per
  target — memoize a per-target heading set
  (`HashMap<rel_path, HashSet<heading>>`) across all links in the
  invocation
- [ ] A/B timing recorded here (bench-e2e.sh / hyperfine method from
  iter-189 task 6): `find --broken-links` with and without the anchor
  branch, indexed and unindexed, on the bench vault; numbers in the table
  before ticking
- [ ] AC: indexed path within noise; unindexed path bounded by the number
  of distinct anchored-link targets

| bench | pre-anchor (iter-189 head) | branch | delta |
| --- | --- | --- | --- |
| find-broken-links (disk) | TBD | TBD | TBD |
| find-broken-links (--index) | TBD | TBD | TBD |

### 5. HYALO006 interaction — decide and record [0/2]

- [ ] Decide whether broken anchors surface in HYALO006
  (link_lint.rs:105-143 currently reports target misses only; catalog
  entry at hyalo-mdlint engine.rs:50/:58/:168-175). Proposal: NOT this
  iteration — keep HYALO006 target-only, let anchor semantics soak one
  release behind `find --broken-links` before lint/CI gates on them
  (mirrors DEC-058's warn-first caution); severity-configurable inclusion
  (own rule id vs. HYALO006 sub-severity) is explicitly a follow-up
  decision
- [ ] Record **DEC-061** either way (included or explicitly not, with
  rationale); if excluded, the HYALO006 description text and README lint
  section state that anchors are not checked

### 6. mv fragment preservation — verify and lock [0/2]

- [ ] Verified at plan time that mv does NOT drop fragments (see Goal
  constraints; L-1/L-2/L-7 history in
  [[reviews/link-handling-review-2026-07-18]] resolved in iter-184).
  Re-verify on the branch after task 1's parse changes: the span splice
  invariant (target_end before `#`) is what guarantees preservation —
  if any regression appears, fixing it is IN scope for this iteration
- [ ] CLI-level e2e (new): vault with `[[Foo#Real]]`, `[t](Foo.md#Real)`,
  and a frontmatter `"Foo#Real"` wikilink; `hyalo mv Foo.md --to Bar.md`
  rewrites all three to `Bar` targets with `#Real` intact (single and
  batch mv, dry-run and apply)

### 7. e2e matrix [0/8]

All under crates/hyalo-cli/tests/e2e/ (find.rs / links.rs / lint.rs /
mv.rs as appropriate):

- [ ] `[[Foo#Real]]` where `Foo.md` has `## Real` → not broken
- [ ] `[[Foo#nope]]` → broken-ANCHOR category (target resolves)
- [ ] `[[Nope#x]]` → broken-TARGET only, no anchor finding on the same
  link
- [ ] `[[Foo#^block]]` → skipped (valid, never reported)
- [ ] Markdown fragment variants: `[t](Foo.md#Real)`,
  `[t](<my dest.md#head>)` (angle form, PR #220), percent-encoded
  `[t](my%20dest.md#my%20heading)` — all resolve target AND anchor
- [ ] Fragment-only `[[#h]]` / `[t](#h)` remain non-links (no findings,
  no graph entries)
- [ ] Index parity: every case above identical with and without
  `--index`; old-snapshot fixture falls back to disk scan with the
  warning (task 1)
- [ ] mv preservation e2e (task 6) green in the same matrix run

### 8. Docs, decisions & retrospective [0/4]

- [ ] DEC-060 (anchor-match convention + fragment percent-decoding) and
  DEC-061 (HYALO006 anchor stance) recorded in [[decision-log]]
- [ ] README (`find --broken-links` section), CHANGELOG (shape bump +
  rebuild note + new category), `templates/rule-knowledgebase.md` if the
  HYALO006 wording changes; no release
- [ ] Update the deferred annotations: iter-188 task 3 "deferred:" entries
  and the review's L-21 disposition line flipped to resolved-in-190
- [ ] KB edits via `hyalo set`/`read`/`lint` per CLAUDE.md

## Acceptance Criteria

- [ ] ONE `Link` wire-shape change lands with all of: parse-side fragment
  capture, exact-heading matcher, distinct broken-anchor category, perf
  guard, CHANGELOG rebuild note — no partial subset
- [ ] Old `.hyalo-index` snapshots fall back to disk scan with the
  existing warning (index.rs:743-748); e2e proves it
- [ ] `find --broken-links` distinguishes broken-target vs broken-anchor;
  never both on one link; JSON and text output covered
- [ ] `links fix` headline `broken`/`fixable`/"Apply N fixes" unchanged by
  broken anchors (e2e guard)
- [ ] Rewrite spans untouched: mv/links-fix preserve fragments byte-exact
  (unit + CLI e2e)
- [ ] Indexed anchor validation performs zero file reads; unindexed reads
  only anchored-link targets; A/B numbers recorded in this plan
- [ ] DEC-060 and DEC-061 recorded; docs in sync
- [ ] `cargo fmt` / `clippy --workspace --all-targets -- -D warnings` /
  `cargo test --workspace -q` clean
