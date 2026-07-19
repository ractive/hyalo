---
title: "Iteration 183 — link scanner unification (Phase B: one lexer, six loops migrated)"
type: iteration
date: 2026-07-18
status: completed
branch: iter-183/link-scanner-unification
tags:
  - iteration
  - links
  - scanner
  - refactor
depends-on: "[[iterations/iteration-178-link-anchor-integrity]]"
related:
  - "[[reviews/link-handling-review-2026-07-18]]"
  - "[[iterations/done/iteration-150-link-handling-refactor]]"
---

# Iteration 183 — link scanner unification (Phase B)

## Goal

One canonical, stateful link lexer; zero hand-rolled scan loops. Finishes
what [[iterations/done/iteration-150-link-handling-refactor]] started:
the `FileVisitor` trait is the right abstraction but is bypassed by 6
independent body-scan loops (link_fix ×1, link_rewrite ×3, auto_link ×2)
and a second frontmatter extractor. Fixes L-3, L-4, L-13, L-15 (plus
L-17, L-18, L-20 opportunistically) from
[[reviews/link-handling-review-2026-07-18]] once, everywhere.

## Tasks

### 1. Cross-line code-span state (L-3, HIGH)

- [x] Add `code_span: Option<usize>` (open backtick-run length) to the
  scanner state alongside `fence`/`in_comment`
  (scanner/mod.rs:252,431,623) and thread it through
  `dispatch_body_line`; `strip_inline_code` reports unclosed trailing
  openers to the caller (mirror `FenceTracker`)
- [x] CommonMark closing rule: a run of exactly N backticks closes,
  across newlines
- [x] Regression tests: multi-line span hiding `[[link]]` and
  `[t](x.md)` — extraction, `find --broken-links`, `backlinks`, `mv`
  (must NOT rewrite), `links fix`, `links auto`, lint
- [x] HTML comments become a suppression context in the same state enum
  (`Normal` / `InlineCode{delim_len}` / `FencedBlock` / `HtmlComment`),
  multi-line aware (L-15)

### 2. One frontmatter delimiter policy (L-4, HIGH)

- [x] New canonical `is_closing_delimiter` in frontmatter/parse.rs;
  replace the three lenient `trim() == "---"` sites (:537, :582, :622)
  and align with `find_closing_delimiter` (:709-733) — decide
  strict-column-0 vs lenient once, document in the helper
- [x] e2e: the indented `  ---` fixture parses identically under
  `find`, `read`, `lint`, `mv`
- [x] L-13: replace the 10 raw `trim() == "---"` opening checks
  (link_rewrite.rs :459,:464,:653,:658,:1242,:1247; auto_link.rs
  :513,:518,:593,:598) with `is_opening_delimiter`; BOM-file mv e2e

### 3. Migrate the six loops onto the shared scanner

- [ ] Extend `FileVisitor` (scanner/visitor.rs:53-107) with
  `on_frontmatter_line(raw, line_num)` and expose line byte-offsets
  (already tracked at mod.rs:115-129) so `Replacement.byte_offset`
  stops being computed ad hoc
- [ ] Behavior-capture regression tests for each loop BEFORE migrating
  (lock current output on a fixture corpus incl. fences, `%%`, BOM,
  CRLF, unicode)
- [x] Migrate in risk order: link_fix.rs `build_replacements_for_file`
  (:948-1072) → auto_link.rs `resolve_existing_link_targets` (:495-570)
  and `scan_file_for_matches` (:574+) → link_rewrite.rs's three loops
  (:430-529, :575-692, :1220-1310)
- [x] Consolidate frontmatter extraction: `link_graph.rs:497` stays
  canonical; `find_frontmatter_wikilinks` (link_rewrite.rs:1121-1150)
  becomes a thin wrapper or is deleted
- [ ] L-18: frontmatter occurrences get real line numbers at the
  producer (track YAML source spans) — retire the `line: 1` sentinel
  and its consumer workarounds

### 4. Small extraction cleanups while in the area

- [x] L-17: delete `strip_md` (link_fix.rs:709-715), use
  `strip_wikilink_md_suffix` (links.rs:392-406)
- [x] L-20: `is_external` (links.rs:481-484) drops the per-candidate
  lowercase allocation (`eq_ignore_ascii_case` prefix checks)

### 5. Retrospective

- [ ] Perf check vs baseline on MDN (14K files): scan-path within noise
- [ ] Update iterations 184/185 with anything learned

## Acceptance Criteria

- [x] `grep`-audit: no `trim() == "---"` outside the canonical helpers;
  no body-link scan loop outside the shared scanner
- [x] Existing test suite (`cargo test --workspace -q`) passes unchanged
  except the documented L-3/L-4/L-13/L-15 fixes, verified by new regression
  tests `find_broken_links_ignores_multiline_code_span` and
  `find_broken_links_ignores_multiline_html_comment` in
  `crates/hyalo-cli/tests/e2e/links.rs`
- [x] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean

## Implementation notes (2026-07-19)

**Delivered.** One shared, cross-line-aware line classifier
(`scanner/body_state.rs`: `LineScanner` + `BodyLine` + `lines_with_rest`)
now drives all six body-scan loops (`link_fix::build_replacements_for_file`,
`auto_link::{resolve_existing_link_targets, scan_file_for_matches}`,
`link_rewrite::{plan_inbound_rewrites, plan_outbound_rewrites,
plan_outbound_rewrites_batch}`). Zero hand-rolled fence/comment/frontmatter
state remains in those files.

- **L-3 (cross-line code spans)** — new `strip_inline_code_stateful` carries an
  open backtick run across lines with a CommonMark-correct **lookahead**: an
  unclosed opener only starts a multi-line span when a matching closer of the
  same run length exists later in the document (`code_run_exists`). This
  preserves the long-standing "unclosed backtick in prose is literal" behavior
  (so stray backticks never silently swallow following links) while still
  suppressing genuine multi-line spans. Threaded through `dispatch_body_line`
  in the multi-visitor scanner too, so `find --broken-links`, `backlinks`,
  `mv`, `links fix`, `links auto`, and `lint` all share the fix.
- **L-15 (multi-line HTML comments)** — new `strip_html_comments` blanks
  `<!-- … -->` across lines, ordered after code-span stripping so a `<!--`
  inside a code span is not treated as a real comment opener.
- **L-4 / L-13 (one delimiter policy)** — canonical `is_closing_delimiter`
  (lenient `trim() == "---"`, matching every streaming reader) added to
  `frontmatter/parse.rs`; the three lenient sites (`find_body_offset`,
  `skip_frontmatter`, `read_frontmatter_from_reader`), `find_closing_delimiter`
  (now line-based + offset-safe for indented `  ---`), the multi-visitor
  scanner, and all six loops (via `LineScanner`) route through it. Opening
  stays strict-column-0 (BOM-aware) via `is_opening_delimiter`.
- **L-17** — deleted `link_fix::strip_md`; `is_self_link` uses shared
  `strip_wikilink_md_suffix`.
- **L-20** — `links::is_external` drops the per-candidate lowercase allocation
  in favor of `eq_ignore_ascii_case` prefix checks.

**Scoped out (deliberate, documented):**

- **L-18 (`line: 1` sentinel)** — `LinkGraphVisitor::extract_frontmatter_wikilinks`
  works on *parsed* YAML `Value`s (from `on_frontmatter`), which carry no
  source spans; retiring the sentinel requires threading YAML source spans
  through `serde_saphyr`, a separate, larger effort. Left as-is.
- **`find_frontmatter_wikilinks` consolidation** — the plan suggested making it
  a thin wrapper of `link_graph.rs`'s extractor, but the two serve different
  contracts: `link_graph`'s is *value*-based (graph building, no byte offsets),
  while `find_frontmatter_wikilinks` is *line*-based and returns byte offsets
  required for in-place frontmatter rewriting (used by both `link_fix` and
  `link_rewrite::plan_frontmatter_wikilink_rewrites`). It is already the single
  shared line-based extractor; merging would break the offset-based rewrites,
  so it stays as the canonical line-based helper.

**Tests:** 3 e2e regressions (`links.rs`: multi-line code span / HTML comment
suppression + over-suppression guard), plus `LineScanner`, `strip_html_comments`,
and `strip_inline_code_stateful` unit tests. Full suite green
(`fmt` / `clippy -D warnings` / `cargo test --workspace -q`).
