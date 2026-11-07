---
title: "Iteration 329: the reader honours the protected regions"
type: iteration
date: 2026-11-07
tags: [iteration, reader, links, anchors, ci, performance]
status: completed
branch: iter-329/reader-honours-protected-regions
priority: 1
related: ["[[decision-log]]", "[[backlog/done/link-inventory-ignores-protected-regions]]"]
---

# Iteration 329: the reader honours the protected regions

Four pieces: the reader stops counting raw HTML, `$$` math and code-like elements as links,
headings and tasks (the owner's decision, not verified Obsidian behaviour); `{#custom-id}`
headings are anchors; CI checks that the fuzz targets compile; and the Hub's `lint --fix`
time is explained. Decisions: DEC-402 (reader), DEC-403 (custom id), DEC-404 (bulk
durability), DEC-405 (fuzz compile gate). Snapshot format 12.

## Tasks

### 1. The reader honours the protected regions

- [x] One reader view (`BodySyntax::for_reader`, `reader_line`) shared by the link inventory, `links fix`, `mv`, the outline, title promotion, tasks and `read --section`; no writer changed, no second scanner
- [x] CommonMark checked: a `<div>` ends at a blank line, a `<pre>` at `</pre>`; a closing `</pre>` alone is a condition 7 block; a fence marker inside an HTML block is HTML text for the reader
- [x] Explicit HTML anchors (DEC-353) read a separate markup view (`FileVisitor::on_markup_line`)
- [x] `%%` keeps the reader's answer; no disagreement with `protected.rs` found
- [x] Snapshot format 12, folded into the one CHANGELOG format entry
- [x] Table test (15 constructs x readers and writers, disk and `--index`) fails without the fix
- [x] Baseline differential on MDN, GitHub Docs and the Obsidian Hub: lost links, headings and tasks counted and checked against markdown-it
- [x] Disk and `--index` byte-identical outside the provenance keys; incremental `create-index` equals `--force`
- [x] MDN timing, baseline against new, same session
- [x] Backlog item closed; DEC-381 and DEC-400 updated

### 2. `{#custom-id}` headings

- [x] The id joins the explicit anchors of DEC-353, case-sensitive; heading text and slug unchanged (DEC-403)
- [x] GitHub Docs broken anchors before and after measured, the four that changed read

### 3. Fuzz targets compile

- [x] `fuzz/` checked on stable (compiles today, no change needed); step added to `quality-gates`, compile-only (DEC-405)

### 4. Hub `lint --fix` timing

- [x] Median of 3 on a fresh Hub copy per run, load average recorded, for dcf08299, d1801b5a and this branch
- [x] Root cause: `PerDirectory` still flushes every file; `lint --fix` and `PreparedChangeSet` move to `BulkRewrite` through `Durability::for_rewrites` (DEC-404)
- [x] Tests that fail without the fix; DEC-317 figure replaced by what was measured

### Gates

- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -q`
- [x] Every xtask gate, `generate-npm-packages --check`, `cargo deny check`, `hyalo lint --strict`, npm gate, case-sensitive gate, `lint-kb` with the released binary

### PR #411 review round

- [x] R1 a tag is blanked only if it parses under the CommonMark tag grammar, across lines too
- [x] R2 inline spans are marked with a control byte, tasks also need the raw line to be a task
- [x] R3 a bulk session flushes the first eight notes it really writes; `task` and `types` listed
- [x] R4 DEC-404, CLAUDE.md, templates and CHANGELOG state power-loss exposure; backlog item filed ([[backlog/done/bulk-rewrite-durability-against-power-loss]])
- [x] R5, R6 CHANGELOG and CLAUDE.md say HTML blocks exist in the corpora and document condition 7
- [x] R7 `<a>` content is scanned; R8 `%%` inside HTML is HTML text; R9 a quoted block ends at an unquoted blank line
- [x] R10, R11 no `.expect`, no clone without ranges, stale comments fixed
- [x] 25 000 notes against markdown-it in both directions: 0 lost links
- [x] Incremental `create-index` after edits, deletes and renames equals `--force` and disk
