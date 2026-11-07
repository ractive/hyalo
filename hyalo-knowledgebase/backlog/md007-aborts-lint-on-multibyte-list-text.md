---
type: backlog
title: Upstream MD007 aborts the whole lint run on a multibyte list marker
date: 2026-11-07
status: planned
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
