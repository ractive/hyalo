---
title: "Iteration 178 — link anchor integrity (mv, links fix, code spans)"
type: iteration
date: 2026-07-18
status: planned
branch: iter-178/link-anchor-integrity
tags: [iteration, links, mv, data-safety]
related:
  - "[[dogfood-results/dogfood-v0180-final-pre-release]]"
---

# Iteration 178 — link anchor integrity

## Goal

Anchored wikilinks survive every rewrite path. Fixes the dogfood's highest
finding — **BUG-1** (`mv` misses frontmatter `related` links carrying an
anchor) — plus its data-loss companion **BUG-2** (`links fix --apply`
drops the anchor when repairing a frontmatter link) and **BUG-16**
(line-based code-span detection miscounts links). Pre-existing bugs, not
0.18.0 regressions; planned as the first post-release fix.

## Context

Repro chain from [[dogfood-results/dogfood-v0180-final-pre-release]]:
`hyalo mv decision-log.md --to docs/decision-log-archive.md` rewrites body
links (with anchors) and anchor-less frontmatter entries, but leaves
`related: - "[[decision-log#DEC-041]]"` stale; `links fix --apply` then
"repairs" it to `"[[decision-log-archive]]"`, silently losing `#DEC-041`.
Iter-158 already noted "fragment-anchored fm wikilinks in mv" as deferred —
this closes it.

## Tasks

### 1. `mv` rewrites frontmatter links with anchors (BUG-1, HIGH)

- [ ] Frontmatter wikilink rewriting strips/compares the anchor when
  matching the moved target and re-attaches it after rewrite, same as the
  body path (likely `find_frontmatter_wikilinks` / mv rewrite site)
- [ ] e2e: `related` entries with `#anchor` in single files and lists are
  rewritten; body behavior unchanged (regression tests)

### 2. `links fix` preserves anchors in frontmatter repairs (BUG-2, MEDIUM)

- [ ] Frontmatter repair path carries the `#anchor` through to the fixed
  link, matching the body repair path
- [ ] e2e: broken `"[[old-name#SEC]]"` in `related` repairs to
  `"[[new-name#SEC]]"`

### 3. CommonMark-correct code-span suppression (BUG-16, LOW-MEDIUM)

- [ ] Link extraction treats code spans per CommonMark: spans may contain
  newlines and pair backticks across them; a wrapped span must not flip
  the state for later spans on the continuation line
- [ ] Repro test: `` `[[a-\nb]]` and `[[c]]` `` — `c` suppressed
- [ ] Verify all consumers share the fixed scanner (`find
  --broken-links`, `mv`, `links fix`, `backlinks`, lint HYALO004)

### 4. Fuzzy-fix confidence gate (enhancement)

- [ ] `links fix --min-confidence <f>` (or a raised default threshold for
  `--apply`): fuzzy matches below the bar are reported as "review
  suggested" instead of written — dogfood showed 0.90-confidence
  proposals pointing at the wrong file
- [ ] Per-fix confidence shown in `--apply` output, not only dry-run

### 5. Retrospective

- [ ] Update remaining planned iterations with anything learned

## Acceptance Criteria

- [ ] The full dogfood repro chain (mv → broken-link check → links fix)
  ends with zero broken links and zero lost anchors
- [ ] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean
