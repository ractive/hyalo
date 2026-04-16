---
title: "Dogfood v0.12.0 — Post Iteration 120 (Feature Verification + Multi-KB)"
type: research
date: 2026-04-16
status: active
tags: [dogfooding, verification, multi-kb]
related:
  - "[[dogfood-results/dogfood-v0120-post-iter119]]"
  - "[[iterations/iteration-120-dogfood-v0120-post-iter119-fixes]]"
---

# Dogfood v0.12.0 — Post Iteration 120

Binary: `hyalo 0.12.0` (built from source, main branch post-iter-120 merge).
KBs tested: Own KB (248 files), MDN Web Docs (14,245 files, indexed), GitHub Docs (3,520 files).

## Iter-120 Feature Verification

### `--fields outline` alias — WORKING

`find --fields outline` produces identical output to `--fields sections`. Tested standalone and combined with other fields on both own KB and GitHub Docs (3,520 files).

### `--stemmer` ISO 639-1 codes — WORKING

- `--stemmer en` → accepted (English stemmer)
- `--stemmer de` → accepted (German stemmer)
- `--stemmer fr` → accepted (French stemmer, tested on MDN with indexed search)
- `--stemmer EN` → accepted (case-insensitive)
- `--stemmer xx` → clear error: `unknown stemming language "xx"`

### `properties rename --dry-run` — WORKING

Tested on own KB (248 files, 248 modified) and GitHub Docs (3,521 scanned, 7 modified for `heroImage → hero_image`). Shows `dry_run: true`, file list, and no files are actually changed. JSON output is clean. Text output shows the skipped list which is verbose on large KBs (see UX-1 below).

### `tags rename --dry-run` — WORKING

Tested on own KB (91 modified, 157 skipped for tag rename). Correctly shows `dry_run: true` and affected files without writing.

### `create-index` overwrite note — WORKING

First `create-index` has no note. Second run includes `"note": "replaced existing index"` in JSON output.

### Lint parse error hint — WORKING

On GitHub Docs, `lint` correctly detects the unclosed frontmatter in `code-security/concepts/index.md` and the hint shows an executable command: `hyalo lint --limit 0 --dir ../docs/content/ --format json`. This is the iter-120 fix — previously it showed a non-executable comment.

## Bug Regression Testing

### BUG-1 (prior): `mv --dry-run` link rewrites — NOT A BUG (confirmed)

Confirmed again: bare wikilinks (`[[name]]`) are intentionally skipped by `plan_mv` because they use name-based resolution. The dry-run correctly reports 0 rewrites when all links are bare.

### BUG-3 (prior): MDN index load dominates query time — UNCHANGED

MDN indexed query: ~0.67s (index deserialization dominated). Unindexed BM25: ~3.86s. The index still provides ~6x speedup for search. The 113 MB index file size is unchanged.

### Boolean operator warning — STILL FIXED

`find "TODO AND FIXME"` returns results without spurious warnings.

### Non-existent view error — STILL WORKING

`find --view nonexistent` gives clear error with `tip: run 'hyalo views list'`.

### Schema validation on `set` — STILL WORKING

`set --property "status=banana" --validate --dry-run` correctly rejects with did-you-mean suggestion.

## Bugs Found

No new bugs found in this session.

## UX Issues

### UX-1: `properties rename --dry-run` text output includes full skipped list (LOW)

When running `properties rename --from heroImage --to hero_image --dry-run --dir ../docs/content/`, the text output includes every single skipped file (3,514 files) which makes the output enormous and hard to scan. The useful information (7 modified files) is buried at the top.

The JSON output has the same issue — the `skipped` array contains all 3,514 non-matching files. For a bulk operation, the user cares about what WILL change, not what won't. Consider:
- Omitting the skipped list from text output entirely
- Truncating to a count in JSON (e.g., `"skipped_count": 3514`)
- Or adding `--verbose` to opt-in to the full list

### UX-2: `stale-in-progress` view found genuinely stale iteration (data quality note)

The `stale-in-progress` view correctly identified `iteration-112` with 2 unchecked tasks ("Update README with limit bypass and show alias" and "Create iteration file"). This is a data quality finding, not a tool bug — but it demonstrates the view's value for housekeeping.

## What Worked Well

### All iter-120 features work correctly across all KBs
Every feature verified on both own KB and external KBs (MDN 14K files, GitHub Docs 3.5K files). No cross-KB compatibility issues.

### BM25 ranking remains excellent
- MDN: "CSS grid layout" → CSS grid module page first (score 16.73)
- GitHub Docs: "pull request merge conflict" → merge conflicts page first (score 18.09)
- Own KB: "snapshot index" → iter-47 first (score 5.39)

### Error messages are clear and actionable
- Empty string search: `body pattern must not be empty; omit the pattern to match all files`
- Unknown stemmer: `unknown stemming language "xx"` with suggestion
- Unknown view: clear error + tip to run `views list`
- Schema validation: rejects invalid values with did-you-mean

### Views are a powerful housekeeping tool
7 views defined, all working. `stale-in-progress` found a genuinely stale iteration. `completed-with-todos` and `orphans` work for vault hygiene.

### External KB handling is robust
GitHub Docs' unclosed frontmatter file is cleanly skipped with a warning, not crashing 3,520 other files. MDN's 14,245 files work seamlessly with the snapshot index.

## Performance

| Command | Own KB (248) | Own KB (indexed) | MDN (14,245) | MDN (indexed) | GH Docs (3,520) |
|---|---|---|---|---|---|
| `find --limit 1` | 20ms | 19ms | — | — | — |
| BM25 search | 68ms | 19ms | 3.86s | 666ms | 944ms |
| `summary` | 31ms | — | 1.27s | — | 366ms |
| `property filter` | 21ms | — | — | — | — |

No regressions compared to the prior dogfood report. The own KB indexed search (19ms) is notably faster than unindexed (68ms) — a ~3.5x speedup even on a small vault.
