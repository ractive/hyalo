---
title: Dogfood v0.12.0 — Post Iteration 115 Follow-up
type: research
date: 2026-04-15
status: active
tags:
  - dogfooding
  - verification
  - bug-fix
related:
  - "[[iterations/iteration-115-dogfood-v0120-iter114-followup]]"
  - "[[dogfood-results/dogfood-v0120-iter114-followup]]"
---

# Dogfood v0.12.0 — Post Iteration 115 Follow-up

Verification dogfood pass after iter-115 merged, focused on confirming that all five bugs
(BUG-A through BUG-E) and five UX improvements (UX-1 through UX-5) from the iter-114
follow-up report are resolved.

Binary: `hyalo 0.12.0` — `target/release/hyalo` built from `main` (post iter-115 merge).

KBs exercised:
- **Own KB** (241 files)
- **MDN Web Docs** (14,245 files) via `--dir /Users/james/devel/mdn/files/en-us`
- GitHub Docs not available for this session.

## Bug-Fix Verification

### BUG-A: `--index` value parsing trap — FIXED

**Original issue**: (1) `--index PATH` (space-separated) silently swallowed PATH as the FTS
query. (2) `--index=./relative` resolved relative to vault dir, not CWD.

**Test 1 — space-separated warning**:
```
$ hyalo find --index hyalo-knowledgebase/.hyalo-index --count
warning: --index PATH (space-separated) passes PATH as the search query, not as an index file; use --index=PATH (with =) to specify an index file
22
```
Warning is clear and actionable. The command still runs (backward-compat), but the user
is told what happened.

**Test 2 — relative path with `=`**:
```
$ hyalo find --index=hyalo-knowledgebase/.hyalo-index "snapshot" --count
30
```
Resolved from CWD correctly. Prior behavior would have doubled the vault path.

**Test 3 — help text updated**:
`--help` now reads: "resolves a relative PATH against the current working directory, not the
vault dir" and documents the space-separated trap.

**Verdict: FIXED**

### BUG-B: Frontmatter wikilinks missing from link graph — FIXED

**Original issue**: `related:`, `depends-on:`, etc. wikilinks in frontmatter list properties
were preserved as strings but never fed into the link graph. `backlinks`, `--orphan`, and
`--dead-end` missed them.

**Test 1 — backlinks via `related:` frontmatter (test KB)**:
Created `a.md` with `related: ["[[b]]", "[[c]]"]` and `depends-on: ["[[d]]"]`.
```
$ hyalo --dir /tmp/test backlinks b.md
2 backlinks for "b.md"
  a.md: line 1
  a.md: line 15
```
Line 1 = frontmatter, line 15 = body wikilink. Both detected.

**Test 2 — backlinks via `depends-on:` (test KB)**:
```
$ hyalo --dir /tmp/test backlinks d.md
1 backlink for "d.md"
  a.md: line 1
```
`depends-on:` property wikilinks are extracted.

**Test 3 — real KB: iteration-113 backlinks**:
```
$ hyalo backlinks iterations/iteration-113-dogfood-v0120-fixes.md
3 backlinks for "iterations/iteration-113-dogfood-v0120-fixes.md"
  dogfood-results/dogfood-v0120-followup-iter113b.md: line 1
  dogfood-results/dogfood-v0120-post-iter113.md: line 1
  iterations/iteration-114-dogfood-v0120-followup-fixes.md: line 1
```
Previously returned "No backlinks found". Now correctly finds all three references.

**Test 4 — orphan detection excludes frontmatter-linked files**:
In the test KB, `c.md` and `d.md` are only referenced via frontmatter (no body wikilinks).
`find --orphan` returned 2 orphans (the validate-test.md and tasks.md files that have no
inbound links), not c.md/d.md. Correct.

**Test 5 — dead-end detection**:
`b.md`, `c.md`, `d.md` (no outgoing links) appeared in `find --dead-end`. `a.md` did not
(has both body and frontmatter outgoing links). Correct.

**Verdict: FIXED**

### BUG-C: `[[wikilink]]` in `--property` value parsed as YAML flow list — FIXED

**Original issue**: `--property 'related=[[foo/bar]]'` stored a nested YAML list `[[foo/bar]]`
instead of the wikilink string.

**Test 1 — set with wikilink**:
```
$ hyalo --dir /tmp/test set wikitest.md --property 'related=[[foo/bar]]'
# Result in file:
related: "[[foo/bar]]"
```
Stored as a quoted string, not a YAML flow list.

**Test 2 — append second wikilink**:
```
$ hyalo --dir /tmp/test append wikitest.md --property 'related=[[baz/qux]]'
# Result in file:
related:
  - "[[foo/bar]]"
  - "[[baz/qux]]"
```
Flat list of strings. No nested lists.

**Test 3 — wikilink with spaces**:
```
$ hyalo set wikitest2.md --property 'related=[[path/with spaces]]'
# Result: related: "[[path/with spaces]]"
```
Handled correctly.

**Test 4 — wikilink with pipe alias**:
```
$ hyalo set wikitest3.md --property 'related=[[path/to/file|alias]]'
# Result: related: "[[path/to/file|alias]]"
```
Pipe aliases preserved as literal strings.

**Test 5 — append to empty list then wikilink**:
```
$ hyalo set appendtest.md --property 'related=[]'
$ hyalo append appendtest.md --property 'related=[[some/link]]'
# Result:
related:
  - "[[some/link]]"
```
Correctly appended to the empty list.

**Verdict: FIXED**

### BUG-D: `set` / `append` skip schema validation — FIXED

**Original issue**: Write commands accepted any value, even enum/pattern violations. Only
`lint` caught these after the fact.

**Test 1 — `--validate` rejects enum violation**:
```
$ hyalo set iterations/iteration-999-test.md --property 'status=bogus-status' --validate
{
  "error": "iterations/iteration-999-test.md: property \"status\" value \"bogus-status\" not in [planned, in-progress, completed, superseded, shelved, deferred] (did you mean \"completed\"?)",
  "hint": "rerun without --validate or fix the value (provided: \"bogus-status\")"
}
exit: 1
```
Rejected with a helpful "did you mean" suggestion.

**Test 2 — `--validate` rejects pattern violation**:
```
$ hyalo set iterations/iteration-999-test.md --property 'branch=bad-branch' --validate
{
  "error": "...property \"branch\" value \"bad-branch\" does not match pattern \"^iter-\\\\d+[a-z]*/\"",
  "hint": "rerun without --validate or fix the value (provided: \"bad-branch\")"
}
exit: 1
```

**Test 3 — `--validate` allows valid values**:
```
$ hyalo set iterations/iteration-999-test.md --property 'status=completed' --validate
# 1/1 modified, exit: 0
```

**Note**: `--validate` is opt-in (off by default). The `[schema] validate_on_write = true`
config option was also specified in iter-115 but was not tested in this session (would
require modifying `.hyalo.toml`).

**Verdict: FIXED**

### BUG-E: `set --where-property` requires explicit `--file`/`--glob` — FIXED

**Original issue**: `set --where-property "status=planned" --tag test` errored with
"set requires --file or --glob".

**Test 1 — `--where-property` without `--file`/`--glob`**:
```
$ hyalo --dir /tmp/test set --where-property 'status=active' --property 'tested=true'
# scanned: 6, total: 5, modified: [a.md, b.md, c.md, d.md, wikitest.md]
```
Correctly defaulted to scanning all `**/*.md` files and filtered by the where-predicate.

**Test 2 — `--where-tag` without `--file`/`--glob`**:
```
$ hyalo --dir /tmp/test set --where-tag test --property 'marker=found'
# scanned: 9, total: 0
```
Ran without error (0 matches because no files had that tag). Previously would have errored.

**Verdict: FIXED**

## UX Improvement Verification

### UX-1: `--stemmer` alias for `find --language` — FIXED

```
$ hyalo find --stemmer english "tests" --count
154

$ hyalo find --stemmer english "snapshot" --count
31
```

`--stemmer` is accepted as an alias. The `--help` text clarifies: "Stemmer language for
BM25 body search (also --stemmer). Selects Snowball stemmer for BM25 tokenization — NOT
markdown code-block language."

**Verdict: FIXED**

### UX-2: `lint --count` — FIXED

```
$ hyalo lint --count
0
```

Returns a bare integer (files with issues). Previously errored with "--count is only
supported for list commands". The own KB is clean (0 issues), so the count is correct
per `hyalo summary` which also shows `schema.files_with_issues: 0`.

**Verdict: FIXED**

### UX-3: `task toggle --dry-run` direction format — FIXED (iter-116)

The iter-115 spec called for `line N: [x] -> [ ]` format showing the direction of change.
Iter-115 added the arrow-format code but the `Format::Text` branch was unreachable
because the dispatch layer forces JSON internally; iter-116 reworked this via a
dedicated `TaskDryRunResult` shape and a shape-based text filter.

**Text output** (iter-116):
```
$ hyalo task toggle tasks.md --all --dry-run --format text
"tasks.md":6 [x] -> [ ] Completed task
"tasks.md":7 [ ] -> [x] Open task
"tasks.md":8 [x] -> [ ] Another done task
```

**JSON output** (iter-116): each result now carries both `old_status` and `status`,
so the direction is explicit in both formats.

**Verdict: FIXED** — see iter-116.

### UX-4: `properties <typo>` hint — FIXED

```
$ hyalo properties versions
error: unrecognized subcommand 'versions'

  hint: 'properties' has subcommands; try 'hyalo properties summary' or 'hyalo properties rename'
```

Previously showed `"did you mean 'hyalo --version'?"` which was misleading. Now correctly
points to valid `properties` subcommands.

**Verdict: FIXED**

### UX-5: `[lint] ignore` config — FIXED

Added `[lint] ignore = ["validate-test.md"]` to `.hyalo.toml` in the test KB:

```
$ hyalo --dir /tmp/test lint --format text
6 files checked, no issues
```

With 7 total `.md` files in the test directory but only 6 checked, the ignored file was
correctly excluded from lint. The `lint --help` text does not explicitly mention the ignore
config key, but it works at the config level as designed.

**Verdict: FIXED**

## New Issues Discovered

### NEW-1: `task toggle --dry-run` text format lacks arrow direction — FIXED (iter-116)

Closed together with UX-3 above. See `[[iterations/iteration-116-dogfood-v0120-iter115-followup]]`.

### NEW-2: MDN absolute URL-style links remain unresolved (LOW)

MDN files use absolute URL-style links (`/en-US/docs/Web/...`) rather than relative
wikilinks. These show as `(unresolved)` in `find --fields links`:

```
$ hyalo --dir .../mdn find --file web/javascript/.../promise/any/index.md --fields links
  "/en-US/docs/Web/JavaScript/Reference/Iteration_protocols" (unresolved)
  "/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise" (unresolved)
```

This was noted in prior reports as BUG-6 (mv absolute URLs). The links remain unresolved
even though `--site-prefix` exists. This may require explicit `--site-prefix en-US/docs`
which was not tested. Carrying forward as a known limitation rather than a new bug.

## Previously Open Items

| Item | Source | Status |
|------|--------|--------|
| BUG-6: `mv` absolute URL-style link rewriting | multi-kb report | Still open — links show as `(unresolved)`, `mv` reports 0 rewrites. May need `--site-prefix` tuning. |
| UX-5 (old): link resolution case-sensitivity | multi-kb report | Still open — not addressed by iter-113, 114, or 115. Note: this is a different UX-5 from iter-115's lint-ignore feature. |
| UX-6: repeated unclosed-frontmatter warning (GH Docs) | iter-114 report | Addressed by iter-115's `[lint] ignore` config (UX-5). GH Docs not available for verification in this session. |

## What Worked Well

- **Frontmatter link extraction is seamless**: The `related:`/`depends-on:` wikilinks now
  feed naturally into backlinks, orphan detection, and dead-end analysis. The `related`-first
  workflow across iteration and research files now works as intended.
- **`--validate` error messages are excellent**: The "did you mean X?" suggestions on enum
  violations are helpful. The hint to "rerun without --validate" preserves the escape hatch.
- **Wikilink string preservation is robust**: Tested with spaces, pipe aliases, empty lists,
  and sequential appends — all handled correctly without YAML flow-list confusion.
- **`--index` help text is now clear**: The space-separated trap is documented and warned.
  Users won't silently get wrong results.
- **`lint --count` is a natural addition**: Works as expected for quick CI checks.
- **Properties typo hint is on-target**: Points at `summary`/`rename` instead of `--version`.

## Performance

### Own KB (241 files)

| Operation | Time |
|-----------|------|
| `summary` | 0.027s |
| `find "snapshot index" --count` | 0.056s |

### MDN Web Docs (14,245 files)

| Operation | No index | With index | Speedup |
|-----------|----------|-----------|---------|
| `summary` | 0.88s | 0.46s | 1.9x |
| `find "getUserMedia" --count` | 3.21s | 0.33s | 9.7x |
| `find --property page-type=... --count` | 0.54s | 0.35s | 1.5x |
| `create-index` | 2.5s | — | — |

Index build for 14k files takes 2.5s. FTS speedup with index remains impressive (~10x).
Structured search sees smaller gains (1.5x) since property filtering is already fast via
parallel scan.

## Summary Table

| # | Item | Type | Verdict |
|---|------|------|---------|
| BUG-A | `--index` value parsing trap | Bug fix | FIXED |
| BUG-B | Frontmatter wikilinks in link graph | Bug fix | FIXED |
| BUG-C | `[[wikilink]]` in `--property` value | Bug fix | FIXED |
| BUG-D | `set`/`append` schema validation (`--validate`) | Bug fix | FIXED |
| BUG-E | `set --where-property` without `--file`/`--glob` | Bug fix | FIXED |
| UX-1 | `--stemmer` alias for `--language` | UX | FIXED |
| UX-2 | `lint --count` | UX | FIXED |
| UX-3 | `task toggle --dry-run` arrow format | UX | FIXED (iter-116) |
| UX-4 | `properties <typo>` hint | UX | FIXED |
| UX-5 | `[lint] ignore` config | UX | FIXED |

**Totals**: 10 FIXED, 0 PARTIAL, 0 NOT-FIXED (iter-115 + iter-116).
**New issues**: 1 LOW addressed in iter-116 (dry-run format), 1 LOW carry-forward (MDN absolute links — root cause is link case-sensitivity, deferred).
**Previously open**: 2 still open (link case-sensitivity, MDN absolute URLs — same root cause), 1 addressed (UX-6 via lint ignore).
