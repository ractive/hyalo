---
title: "Iteration 219 — list splice, mixed-EOL honesty, and frontmatter budget truth"
type: iteration
date: 2026-08-23
status: planned
branch: iter-219/list-splice-and-write-polish
tags: [iteration, frontmatter, write-path]
related:
  - "[[dogfood-results/dogfood-v0210-pre3-fix-waves-207-214]]"
  - "[[iterations/iteration-214-frontmatter-format-preservation]]"
---

# Iteration 219 — list splice, mixed-EOL honesty, and frontmatter budget truth

## Goal

Extend [[iterations/iteration-214-frontmatter-format-preservation]]'s
minimal-diff guarantee (`hyalo-core/src/frontmatter/splice.rs`) to list
mutations, make the two dishonest write paths honest (mixed line endings,
fallback-trigger docs), and fix the frontmatter size budget that
contradicts its own documentation.

## Context

From [[dogfood-results/dogfood-v0210-pre3-fix-waves-207-214]]:

- **NEW-5 (MEDIUM-HIGH)**: `append` and `remove <key>=<value>` re-serialize
  the whole touched list: one appended `redirect_from` entry churns 361 of
  406 GH Docs files >1 line (worst: `admin/index.md`, 118 lines — entries
  over ~80 cols get refolded into `>-` block scalars). Flow lists explode
  to block style (`tags: [iteration, demo]` → 4 lines on adding a tag —
  hits own-KB usage directly). DEC-080's defect relocated from "whole
  frontmatter" to "whole touched list". `set` is at 406/406 one-line
  diffs; that is the bar.
- **NEW-7 (MEDIUM)**: mixed line endings are documented (DEC-081, `set
  --help`, `remove --help`) as a *warned* fallback trigger. No warning
  fires and untouched lines lose their `\r` (repro: file with `\r\n` on
  lines 2 and 4, `\n` elsewhere; `set` of an unrelated key → CR count
  2 → 0, stderr empty).
- **NEW-8 (MEDIUM)**: help says frontmatter is limited to "64 KiB / 2000
  lines"; the real ceiling is 8,192 bytes of total scalar content, and
  breaching it leaks parser internals: `budget breached: ScalarBytes {
  total_scalar_bytes: 8205 }`. GH Docs `admin/index.md` (7,961 bytes) is
  ~40 redirect entries from being unreadable. Same leak family:
  `duplicate mapping key: …, set DuplicateKeyPolicy in Options if
  acceptable`, `budget breached: Anchors { anchors: 1 }`.
- **NEW-16 residue (LOW)**: body-less files without a trailing newline
  gain one on `set` (6 / 406 GH Docs files); DEC-081's fallback-trigger
  list names anchors/aliases and directives, which actually hard-error
  and never reach the fallback; dotted `--property versions.fpt=X`
  silently creates a literal top-level key named `versions.fpt` beside an
  existing `versions:` map; `set` type inference silently retypes `'42'`
  (string) → `42` (number).

## Tasks

- [ ] Splice within lists: `append` inserts one item's line span into a
      block list without re-emitting siblings; `remove <key>=<value>`
      deletes only the matching item's span. Preserve flow style on flow
      lists (append inside the brackets) unless the new item cannot be
      represented inline, in which case fall back with the DEC-081
      warning
- [ ] Mixed line endings: either preserve per-line endings through the
      splice, or actually fire the documented fallback warning — no
      silent churn. Decide and record which in the decision log
- [ ] Raise the scalar-content budget to match the documented 64 KiB (or
      an explicitly chosen limit), and wrap all budget/duplicate-key/
      anchor parser errors in actionable hyalo-voice messages that name
      the file and the limit — no leaked `ScalarBytes {…}` /
      `DuplicateKeyPolicy` internals
- [ ] Files ending exactly at `---` with no trailing newline stay
      byte-identical outside the intended change
- [ ] Reject a dotted `--property a.b=x` with an error when a top-level
      map `a` exists (pointing at the collision); document that path
      syntax is unsupported
- [ ] Advisory note when type inference changes a value's YAML type
      (string → number/bool), reusing the existing enum/date advisory
      mechanism
- [ ] Correct DEC-081's fallback-trigger list to the real set (`? key`
      syntax, top-level flow mappings, invalid UTF-8, and whatever
      mixed-EOL decision is made); sync `set`/`remove`/`append` help
- [ ] e2e: GH Docs-style corpus asserting one-line diffs for
      append/remove on block and flow lists; mixed-EOL file; 60 KiB
      frontmatter parse; no-trailing-newline file; dotted-key rejection

## Acceptance criteria

- [ ] `append`/`remove <key>=<value>` on the 406-file GH Docs corpus:
      0 files change more than the intended line span (was 361)
- [ ] Mixed-EOL file: untouched lines keep their endings, or a fallback
      warning names mixed line endings — never silent churn
- [ ] `admin/index.md` + 40 appended redirect entries parses and splices
- [ ] No parser-internal type names appear in any user-facing error

## Non-goals

- Nested-key path syntax support (only the collision guard)
- Frontmatter reformatting features (`fmt`-style canonicalization)
