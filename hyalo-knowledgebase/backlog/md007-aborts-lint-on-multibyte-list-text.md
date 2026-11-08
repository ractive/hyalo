---
type: backlog
title: Upstream MD007 aborts the whole lint run on a multibyte list marker
date: 2026-11-07
status: completed
priority: medium
origin: PR 412 review, 2026-11-07
---

## Problem

With MD007 enabled (`[lint.rules.MD007] enabled = true`), `hyalo lint` aborts the whole run on the Obsidian Hub:
exit 134 (SIGABRT, `panic = "abort"`) and no output. It does so on every binary, baseline included.

```text
panicked at mdbook-lint-rulesets-0.16.2/src/standard/md007.rs:268:79:
start byte index 4 is not a char boundary; it is inside '⚠' (bytes 3..6 of string)
```

Exit 134 is outside the 0 / 1 / 2 contract (DEC-307), and one note takes down the findings for all the others.

## Notes

- Found while enabling every stock rule for the iteration 330 differential; the Hub run there left MD007 off.
- Upstream bug: the rule slices a line at a byte offset. Options are a bounded fix upstream, a vendored guard, or
  running each rule under `catch_unwind` so a rule panic becomes a per-note diagnostic (`panic = "abort"`
  forbids that today).
- Not fixed in iteration 330.

## Outcome (iteration 337)

Fixed on hyalo's side, see DEC-412 and [[iterations/iteration-337-backlog-sweep]]. The cause is wider than a multibyte
marker: upstream counts a leading **tab** as four columns, then slices the line at that count as if it were a byte
offset (`&line[indent..]`, `md007.rs` line 268 in 0.16.2, the latest release). A tab-indented bullet with multibyte
text panics; with ASCII text the fix silently drops the item's first characters. `hyalo-mdlint` now hands MD007 a view
in which the text of a tab-indented bullet is emptied (`md007_view`, like `md034_view`); MD010 still reports the tab.

Obsidian Hub (a copy of checkout `a56b648b`, `[lint.rules.MD007] enabled = true`, plain `lint`): exit 134 with no
output before, exit 1 with the findings of all 6 520 notes after (MD007 alone: exit 0, 3 457 warnings over 71 files).
Both new tests fail with the guard removed (the unit test with the upstream panic, the end-to-end test with exit 101).
