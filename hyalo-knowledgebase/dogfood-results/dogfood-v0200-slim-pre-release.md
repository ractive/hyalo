---
title: Dogfood v0.20.0-pre — slim pass over anchors, HYALO006, envelopes
type: research
date: 2026-07-19
status: active
tags: [dogfooding, links, lint, anchors]
related: "[[iterations/iteration-190-link-anchors]]"
---

# Dogfood v0.20.0-pre — slim pass over anchors, HYALO006, envelopes

Slim pre-release pass at main `1f1be9c` (187–190 + #225 + #226 + AUR
enablement). Own KB (347 files, fresh `.hyalo-index`) + two scratch vaults.

## Verified

- **Anchor validation, disk vs `--index` parity**: identical results on the
  own KB (0 broken after the hygiene pass) and on a positive-control vault
  (1 broken-anchor + 1 broken-target on both paths). Text output shows
  `#fragment` and `(broken anchor)` (PR #225 fix confirmed live).
- **HYALO006 as CI gate**: `lint --strict --rule HYALO006` exits 1 on a
  broken wikilink with a clear message; broken *anchors* correctly not
  flagged (DEC-061). `--files-from` scoping does not false-positive on
  links to unscoped-but-existing files.
- **L-11 partial-failure envelope** (`links fix --apply`, one linker in a
  read-only directory): exit 1, `failed: 1`, `failed_fixes` carries the OS
  error string, `applied_fixes` lists only the file actually rewritten
  (verified on disk), untouched file keeps its old text, stderr warning
  emitted.
- **`find --orphan`/`--dead-end` vs `summary`** (PR #226): counts identical
  on the own KB (orphans 78/78, dead-ends 87/87).

## Notes (not bugs)

- `chmod 444` on a *file* does not force a write failure — atomic
  replace needs directory write permission only. Read-only *directory* is
  the correct fixture for partial-failure testing (the e2e suite already
  does this correctly).
- `[[target]]` for `Target.md` is a legitimate case-insensitive stem match
  (resolved, not fixable); a *path-qualified* wrong-case link
  (`[[sub/target]]`) is what lands in the `case_mismatches` bucket.

## Verdict

No findings. Release v0.20.0: **go**.
