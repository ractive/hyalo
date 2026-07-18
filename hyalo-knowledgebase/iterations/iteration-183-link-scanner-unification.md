---
title: "Iteration 183 — link scanner unification (Phase B: one lexer, six loops migrated)"
type: iteration
date: 2026-07-18
status: planned
branch: iter-183/link-scanner-unification
tags: [iteration, links, scanner, refactor]
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

- [ ] Add `code_span: Option<usize>` (open backtick-run length) to the
  scanner state alongside `fence`/`in_comment`
  (scanner/mod.rs:252,431,623) and thread it through
  `dispatch_body_line`; `strip_inline_code` reports unclosed trailing
  openers to the caller (mirror `FenceTracker`)
- [ ] CommonMark closing rule: a run of exactly N backticks closes,
  across newlines
- [ ] Regression tests: multi-line span hiding `[[link]]` and
  `[t](x.md)` — extraction, `find --broken-links`, `backlinks`, `mv`
  (must NOT rewrite), `links fix`, `links auto`, lint
- [ ] HTML comments become a suppression context in the same state enum
  (`Normal` / `InlineCode{delim_len}` / `FencedBlock` / `HtmlComment`),
  multi-line aware (L-15)

### 2. One frontmatter delimiter policy (L-4, HIGH)

- [ ] New canonical `is_closing_delimiter` in frontmatter/parse.rs;
  replace the three lenient `trim() == "---"` sites (:537, :582, :622)
  and align with `find_closing_delimiter` (:709-733) — decide
  strict-column-0 vs lenient once, document in the helper
- [ ] e2e: the indented `  ---` fixture parses identically under
  `find`, `read`, `lint`, `mv`
- [ ] L-13: replace the 10 raw `trim() == "---"` opening checks
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
- [ ] Migrate in risk order: link_fix.rs `build_replacements_for_file`
  (:948-1072) → auto_link.rs `resolve_existing_link_targets` (:495-570)
  and `scan_file_for_matches` (:574+) → link_rewrite.rs's three loops
  (:430-529, :575-692, :1220-1310)
- [ ] Consolidate frontmatter extraction: `link_graph.rs:497` stays
  canonical; `find_frontmatter_wikilinks` (link_rewrite.rs:1121-1150)
  becomes a thin wrapper or is deleted
- [ ] L-18: frontmatter occurrences get real line numbers at the
  producer (track YAML source spans) — retire the `line: 1` sentinel
  and its consumer workarounds

### 4. Small extraction cleanups while in the area

- [ ] L-17: delete `strip_md` (link_fix.rs:709-715), use
  `strip_wikilink_md_suffix` (links.rs:392-406)
- [ ] L-20: `is_external` (links.rs:481-484) drops the per-candidate
  lowercase allocation (`eq_ignore_ascii_case` prefix checks)

### 5. Retrospective

- [ ] Perf check vs baseline on MDN (14K files): scan-path within noise
- [ ] Update iterations 184/185 with anything learned

## Acceptance Criteria

- [ ] `grep`-audit: no `trim() == "---"` outside the canonical helpers;
  no body-link scan loop outside the shared scanner
- [ ] All behavior-capture tests pass unchanged except the documented
  L-3/L-4/L-13/L-15 fixes
- [ ] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean
