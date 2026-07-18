---
title: "Link-handling deep review — 4-layer audit, 15 confirmed defects, consolidation plan"
type: research
date: 2026-07-18
status: active
tags: [review, links, architecture, rust]
related:
  - "[[dogfood-results/dogfood-v0180-final-pre-release]]"
  - "[[iterations/done/iteration-150-link-handling-refactor]]"
  - "[[iterations/iteration-178-link-anchor-integrity]]"
  - "[[reviews/codebase-review-2026-07-10]]"
---

# Link-handling deep review — 2026-07-18

Four parallel rust-developer reviews over the whole link surface
(~12K lines): extraction/parsing (`links.rs`, scanner), resolution/graph
(`link_resolve.rs`, `link_graph.rs`, `case_index.rs`, index), rewriting
(`link_rewrite.rs`, `mv.rs`, `link_write.rs`), repair/auto-link
(`link_fix.rs`, `auto_link.rs`). All CRITICAL findings were confirmed
empirically against the built binary, not just read from source. HEAD:
`e76e89b`.

## Verdict

The user's impression is correct, and the review explains *why* link bugs
keep recurring. [[iterations/done/iteration-150-link-handling-refactor]]
promised "a single canonical representation … one resolver and one writer
shared by every mutator" — but the consolidation stopped at the
lowest-level scanner. Today there are still:

- **6 independent body-scan loops** outside the shared scanner
  (`link_fix.rs` ×1, `link_rewrite.rs` ×3, `auto_link.rs` ×2), each
  re-implementing frontmatter-skip + comment-fence + code-fence +
  inline-code sequencing, with confirmed per-loop divergences;
- **2 frontmatter wikilink extractors** (`link_graph.rs:497` canonical
  YAML-aware; `link_rewrite.rs:1121` hand-rolled `[[`/`]]` scanner);
- **5 semi-independent resolvers** (find's inline block
  `find/mod.rs:735-765`, its near-duplicate `link_fix.rs:371-391`,
  `LinkGraph` key matching, `link_resolve::LinkResolver`,
  `link_fix::resolve_and_classify_link`);
- **2 write paths** (`execute_plans`/`RewritePlan` vs `auto_link`'s
  hand-rolled `apply_matches`) with different TOCTOU guards.

Every recurring bug class of the past months — BOM delimiter, `%%`-fence
ordering, byte/char columns, frontmatter no-ops, and this week's anchor
losses — is an instance of one pattern: **a fix lands in one copy, the
siblings drift.** The shared scanner (`FileVisitor`) exists and is the
right abstraction; it is simply under-adopted.

## Confirmed defects (severity-ranked)

### HIGH

**L-1 — `mv` never rewrites the moved file's own frontmatter self-links.**
`plan_outbound_rewrites` (`link_rewrite.rs:630-826`) and its batch
counterpart (`:1223-1325`) track frontmatter state but never invoke the
frontmatter rewriter for the file being moved; only
`plan_inbound_rewrites` (other files) does. Any plain rename of a file
with a self-referencing frontmatter link leaves it dangling — no anchor
needed. Found while confirming L-2; worse than the dogfood finding.

**L-2 — `mv` skips frontmatter links carrying an anchor (dogfood BUG-1).**
`plan_frontmatter_wikilink_rewrites` (`link_rewrite.rs:1179`) strips the
alias (`split('|')`) but never the `#fragment`, so
`"decision-log#DEC-041" != old_stem` and the entry is silently left
stale. All frontmatter shapes affected (scalar, list, quoted, aliased).
Fix: use the canonical fragment-aware `parse_wikilink` for matching and
reattach fragment+alias on rebuild (helper `split_target_fragment`
already exists, tested, at `link_rewrite.rs:302-307`).

**L-3 — Multi-line code spans are invisible; phantom links get rewritten.**
`strip_inline_code` (`scanner/strip.rs:6-63`) is a pure per-line
function; an unmatched backtick is rewound to literal text and no state
crosses lines, though CommonMark spans close across newlines. A
`[[link]]` inside a multi-line code span is extracted as real
(`find --fields links`) **and rewritten by `mv`** (confirmed via
`--dry-run`) — silent corruption of code samples. All consumers bottom
out in the same stateless function, and the 6 duplicate loops call it
per-line too, so the fix must land in the shared scanner state
(`code_span: Option<usize>` alongside `fence`/`in_comment` in
`scanner/mod.rs:252,623`) *and* the loops must adopt it. Supersedes
dogfood BUG-16 (which caught only the milder later-span-on-line case).

**L-4 — The same file parses differently per subcommand.**
`frontmatter/parse.rs` closing-delimiter policy disagrees: lenient
`trim() == "---"` at :537, :582, :622 vs strict column-0
`find_closing_delimiter` at :709-733. Confirmed: an indented `  ---`
mid-frontmatter closes the block for `hyalo find` but not `hyalo read` —
different frontmatter/body splits for identical bytes. Fix: one
canonical `is_closing_delimiter`, mirroring the existing
`is_opening_delimiter` whose doc comment states exactly this anti-drift
intent.

**L-5 — `mv --index` leaves stale link-graph entries.**
`mutation.rs:122` calls `refresh_entry` (skips the link graph) instead
of `refresh_entry_and_links` (`index.rs:478-488`, already used correctly
by `links fix --apply --index`). Confirmed: after an indexed mv,
`backlinks --index` reports the stale pre-rewrite entry until a full
rebuild. One-line fix. (Root cause of deferred backlog item H-F/H-G.)

### MEDIUM

**L-6 — Case-sensitivity divergence in backlinks/orphan/summary.**
`backlinks.rs:41-45`, `find/mod.rs:824-826`, `summary.rs:293-302` call
raw `LinkGraph::backlinks()`/`all_targets()`/`all_sources()`; only mv's
planner uses `backlinks_case_insensitive()` (`link_graph.rs:205-221`).
Confirmed: same file returns 3 vs 1 backlinks depending on argument
casing; orphan/dead-end counts wrong on case-insensitive vaults.
Caution: don't naively swap — the case-insensitive helper is O(vault)
per call; bake a lowercased-key companion map into `LinkGraph`
(`insert_file_links`, `link_graph.rs:391-458`) for O(1) lookups.

**L-7 — `links fix` drops anchors in frontmatter repairs (dogfood BUG-2).**
`build_replacements_for_file` frontmatter block (`link_fix.rs:997-1033`)
rebuilds from the fragment-stripped `parse_wikilink` output and
reattaches only the alias, never the fragment. The body path preserves
anchors by splicing raw bytes. Two-step data loss with L-2. Fix mirrors
L-2's; add a shared fragment+alias-aware
`rewrite_frontmatter_wikilink_text` helper used by both callers.

**L-8 — `link_fix.rs` toggles `%%` comment state inside code fences.**
`link_fix.rs:1050-1062` checks `is_comment_fence` *before*
`fence.process_line` with no `!fence.in_fence()` guard; a literal `%%`
line in a fenced block desyncs suppression for the rest of the file.
`link_rewrite.rs:482-496` and `auto_link.rs:527-536` order it correctly
— lone diverger, classic sibling drift.

**L-9 — Fuzzy-match phantom-tie rejects unique legitimate matches.**
`LinkMatcher::find_match` (`link_fix.rs:789-824`) seeds `best_score`
with the threshold itself, which acts as a phantom second competitor: a
single real candidate scoring within TIE_DELTA (0.01) above threshold is
rejected as "ambiguous". Confirmed against the real matcher. Fix: seed
with `NEG_INFINITY`, gate on threshold after the loop.

**L-10 — `--apply` writes fuzzy fixes with no confidence tier.**
`links.rs:93-134` applies every `FixPlan` uniformly: `CaseInsensitive`
(1.0) and `FuzzyMatch` (down to 0.8) are written identically. Confirmed
0.896-confidence wrong target (`iteration-132-mv-wikilinks` →
`iteration-02-links.md`, Jaro-Winkler prefix bonus over-rewarding
`iteration-`); a live instance of this false positive already sits in
this KB (iteration-150's related links). Recommend: report FuzzyMatch
in a separate bucket excluded from `--apply` by default (like ambiguous
short-form links), opt-in via flag.

**L-11 — Partial-failure mid-apply discards the result envelope.**
`apply_fixes` (`link_fix.rs:871-932`) computes plans then calls
`execute_plans` once for the whole batch; a failure on file 3-of-5
propagates `Err` after files 1-2 are durably written — no JSON envelope,
caller can't distinguish "nothing written" from "partially written".
Batch `mv` shares the class: `mv.rs:586-599` rolls back renames but not
completed `atomic_write`s.

**L-12 — auto-link word boundaries are ASCII-only.**
`is_word_boundary_byte` (`auto_link.rs:278-283`) treats every non-ASCII
byte as a boundary; confirmed false-positive auto-links glued to CJK
text and across U+2011 non-breaking hyphen. Test coverage is ASCII-only.

**L-13 — BOM-aware frontmatter detection missing in 10 sites.**
`link_rewrite.rs` (:459,:464,:653,:658,:1242,:1247) and `auto_link.rs`
(:513,:518,:593,:598) hand-roll `trim() == "---"`; `link_fix.rs:986`
uses the canonical `is_opening_delimiter`. A BOM-prefixed file being
mv'd fails to recognize its own frontmatter — the exact
iter-158-missed-siblings pattern.

**L-14 — Case-only rename fails in single-file mv.**
`mv.rs:717-738` uses a case-sensitive same-path check; batch mode
already has the canonicalize-based check (`mv.rs:436-440`). `a.md` →
`A.md` on a case-insensitive FS errors in single-file mode.

### LOW

- **L-15** HTML comments are not a suppression context anywhere —
  `<!-- [[x]] -->` is extracted (and rewritable). Add `HtmlComment` to
  the scanner state (multi-line aware, or it becomes another L-3).
- **L-16** Backslash escapes ignored: `\[[not-a-link]]` extracted.
- **L-17** `strip_md` (`link_fix.rs:709-715`) slices `s[len-3..]`
  without a boundary check — panics on multibyte input; currently
  unreachable (all callers pass `.md` paths) but a landmine. Replace
  with the safe duplicate `strip_wikilink_md_suffix` (`links.rs:392`).
- **L-18** Frontmatter link occurrences carry sentinel `line: 1`
  (`link_graph.rs:502`) — the footgun that caused iter-160's no-op bug;
  fix in the producer (track YAML source spans).
- **L-19** `.md`-suffix asymmetry: `parse_wikilink` strips
  (`links.rs:378`), `parse_markdown_link` keeps (`links.rs:461-462`);
  `auto_link.rs:552-557` manually re-strips. Normalize at `Link`
  construction, keep an as-written field.
- **L-20** `is_external` allocates a lowercased `String` per candidate
  on the hot path (`links.rs:481-484`); use `eq_ignore_ascii_case`.
- **L-21** Anchors are stripped at parse (`links.rs:488-490`) and never
  validated — `[[Foo#nonexistent]]` indistinguishable from valid
  everywhere, including `--broken-links`.
- **L-22** No broken-link lint rule exists (HYALO004 unimplemented) —
  link health can't gate CI via `lint --strict`.
- **L-23** `resolve_target` (`discovery.rs:714-842`) never
  percent-decodes markdown targets (`my%20page.md` unresolvable).
- **L-24** `--exclude-target-glob` is case-sensitive while
  `--exclude-title` is case-insensitive (`auto_link.rs:107-124`).
- **L-25** `links fix` dry-run doesn't validate plans against on-disk
  text (apply does) — cross-invocation divergence reproduced.
- **L-26** `create-index --index-file idx.bin` (bare filename) fails
  with `failed to canonicalize parent of output path: ""`.

Ruled out (verified non-bugs): `--site-prefix` behavior matches help in
the resolution layer (the stale text is elsewhere, see iter-177); no
O(n²) hot paths in graph build; no unwrap/expect violations beyond L-17;
no edition-2024 issues; mdlint byte/char columns already fixed via
allowlist.

## Consolidation plan

Phased so surgical fixes aren't hostage to the refactor:

**Phase A — surgical fixes (small PRs, immediately shippable):**
L-1/L-2/L-7 via one shared fragment+alias-aware frontmatter rewrite
helper used by mv and links-fix; L-5 one-liner
(`refresh_entry_and_links`); L-8 ordering guard; L-9 seed fix; L-14
canonicalize check reuse. Locking tests per the agent write-ups
(anchored + aliased + self-link frontmatter fixtures for both mv and
links fix).

**Phase B — scanner unification (fixes L-3/L-4/L-13/L-15 once):**
add `code_span: Option<usize>` cross-line state to
`scan_slice_multi`/`scan_reader_multi`/`dispatch_body_line`; canonical
`is_closing_delimiter`; `on_frontmatter_line` hook + line byte-offsets
exposed on `FileVisitor`; then migrate the six loops in risk order —
`link_fix.rs::build_replacements_for_file` first (gets L-8/L-13 free),
`auto_link.rs`'s two loops, `link_rewrite.rs`'s three loops last (most
call-site-specific; behavior-capture regression tests before migrating).
Delete `find_frontmatter_wikilinks` or make it a thin wrapper.

**Phase C — resolver + writer unification (fixes L-6 at the root):**
extend iter-150's `LinkResolver` into the single entry point
`resolve_link(ctx, link, mode)` with `ResolveMode::Exists`
(find/backlinks/summary) and `ResolveMode::Classify` (links fix), with
case-folding/percent-decoding/anchor-handling as single steps inside;
lowercased companion map in `LinkGraph` for O(1) case-insensitive
lookups; unify `apply_matches` onto `execute_plans` and make the write
path report per-file partial results (L-11); fuzzy confidence tiers
(L-10).

**Phase D — semantics extensions:** anchor validation (L-21),
broken-link lint rule (L-22), backslash escapes + HTML comments as
suppression contexts (L-15/L-16), `.md` normalization at `Link`
construction (L-19), percent-decoding (L-23).

## Relation to existing plans

Planned same day (2026-07-18): Phase A =
[[iterations/iteration-178-link-anchor-integrity]] (re-scoped to the
confirmed root causes, L-1 added); Phase B =
[[iterations/iteration-183-link-scanner-unification]]; Phase C =
[[iterations/iteration-184-link-resolver-writer-unification]]; Phase D =
[[iterations/iteration-185-link-semantics]] — chained via depends-on.
Agent-level write-ups with regression-test sketches are in
`.claude/agent-memory/rust-developer/`
(`pitfall_frontmatter_wikilink_fragment_loss.md`,
`pitfall_fuzzy_match_phantom_tie.md`,
`project_mv_rewrite_review_2026-07-19.md`,
`project_link_resolution_review_2026-07-18.md`).

## Addendum 2026-07-18 — post-iter-176 re-verify findings

Targeted re-verification of the v0.18.0 fixes (after PR #207/#208 merged)
surfaced two parser-side gaps that belong to this review's scope and were
not covered above. Both should be absorbed into the planned phases (A or B),
not fixed pre-release:

- **L-A1 — angle-bracket destinations unsupported by the parser.**
  iter-176's generator now emits CommonMark-valid `[text](<spaced dest.md>)`
  links, but the link scanner stores the target with literal `<>`:
  `find --broken-links` false-positives on the generator's own fresh output
  (every spaced-destination link reported "unresolved"), `backlinks` misses
  them, and the hint then offers `links fix` on spec-valid links.
  Generator/parser split-brain.
- **L-A2 — escaped brackets in link text blind the parser.** A line like
  `[Contains \[test\] brackets](<dest.md>)` is entirely absent from
  `--fields links` and `backlinks` output.

Minor related: `okf index --dry-run` exits 1 on marker-skip vaults with
`changed: 0`, contradicting the documented "non-zero when any index.md
would change" contract (possibly intentional — surface skips in CI — but
undocumented).
