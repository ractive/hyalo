---
title: "Dogfood v0.12.0 — Post Iteration 119 (Multi-KB Session)"
type: research
date: 2026-04-16
status: active
tags:
  - dogfooding
  - verification
  - multi-kb
related:
  - "[[dogfood-results/dogfood-v0120-post-iter118]]"
  - "[[iterations/iteration-118-split-index-flag]]"
---

# Dogfood v0.12.0 — Post Iteration 119 (Multi-KB Session)

Binary: `hyalo 0.12.0` built via `cargo build --release`.
Tested on: Own KB (246 files), MDN Web Docs (14,245 files), GitHub Docs (3,520 files).

## Iter-119 Feature Verification

### `validate_on_write` auto-enable on first type creation — WORKING

Created a fresh `.hyalo.toml` with only `dir = "docs"`, ran `types set mytype --required title`. Output correctly reports `"enable validate_on_write (new schema)"` in `toml_changes`, and the resulting `.hyalo.toml` contains `validate_on_write = true` under `[schema]`.

Also verified `types set --dry-run` on GitHub Docs external KB works correctly — reports what would change without writing.

**Edge case**: When running `types set --dir <subdir>` from outside the project root (the original iter-113 config-lookup bug territory), the feature still works correctly as long as CWD matches the project root.

## Prior Bug Re-verification

### BUG-1 (post-118): `--fields outline` alias — STILL OPEN

`find --fields outline` still fails with: `unknown field "outline": valid fields are all, properties, properties-typed, tags, sections, tasks, links, backlinks, title`.

### BUG-2 (post-118): `--stemmer` ISO codes — STILL OPEN

`find "search" --stemmer en` still fails: `invalid --language value "en": unknown stemming language: "en"`.

### BUG-3 (iter-113): Boolean operator warning — STILL FIXED

`find "AND OR"` correctly warns: `"AND", "OR" were interpreted as boolean operators, leaving an empty query`.

### BUG-D (iter-114): Schema validation on `set` — STILL WORKING

`set --property "status=banana" --validate --dry-run` correctly rejects with did-you-mean suggestion.

### Non-existent view error — STILL WORKING

`find --view nonexistent` gives clear error with tip to run `views list`.

## Bugs Found

### BUG-1: `mv --dry-run` doesn't preview link rewrites (MEDIUM)

`decision-log.md` has 14 backlinks (verified via `backlinks` command). Running `mv decision-log.md --to reference/decision-log.md --dry-run` reports:
```json
{
  "total_files_updated": 0,
  "total_links_updated": 0,
  "updated_files": []
}
```

The dry-run should preview which files would be rewritten and how many links would change, so the user can assess the impact before applying. Currently it only shows the file move itself.

### BUG-2: `properties rename` lacks `--dry-run` (MEDIUM)

`properties rename --from "origin" --to "source" --dry-run` fails: `unexpected argument '--dry-run' found`. This is a bulk mutation that could affect many files — previewing is essential. Same issue affects `tags rename`.

Both `set` and `mv` support `--dry-run`; the rename commands should too for consistency and safety.

### BUG-3: MDN index load dominates query time — potential optimization (LOW)

The MDN snapshot index is 113 MB. Every indexed query takes ~0.67s regardless of complexity, because loading/deserializing the index from disk dominates. Without index, `summary` takes 1.07s (I/O-bound disk scan), so the index saves ~0.4s only for summary-class operations. For BM25 search, index saves significantly more (0.67s vs 3.8s).

Not a bug per se, but the 113 MB index file for 14K files seems large. Could be worth investigating if the serialization format can be more compact, or if lazy/partial loading is feasible.

## UX Observations

### UX-1: `create-index` silently overwrites existing index (LOW)

Running `create-index` when `.hyalo-index` already exists produces no warning — it just rebuilds. A brief note like "replacing existing index (was 2.5s old)" would help users understand what happened, especially if they accidentally run it twice.

### UX-2: `lint --fix --dry-run` on unclosed-frontmatter files gives no fix hint (LOW)

On GitHub Docs, `code-security/concepts/index.md` has unclosed frontmatter. `lint --fix --dry-run` correctly reports the error but doesn't suggest any fix. The hint says "See defined type schemas" which isn't helpful. A better hint would reference the `[lint] ignore` config for known-bad files.

## What Worked Well

### Multi-filter composition is excellent
Tested all combinations: BM25 + `--property` + `--tag` + `--section` + `--task` + `--sort` + `--limit`. Everything composes cleanly. The maximum-filter test (`find "bug" --property type=iteration --property "date>=2026-04-01" --tag dogfooding --section "Tasks" --task done --sort date --reverse --limit 3`) returned exactly the right results.

### BM25 search ranking is spot-on
MDN: "CSS grid layout" → CSS grid module page first (score 16.47). GitHub Docs: "pull request merge conflict" → merge conflicts page first (score 18.09). Own KB: "snapshot index" → iter-47 first. Ranking is consistently excellent across all three KBs.

### External KB compatibility
Both MDN (14K files) and GitHub Docs (3.5K files) work seamlessly. The `--dir` flag, `--ignore-target`, `--site-prefix`, and schema-less operation all work correctly. The unclosed-frontmatter skip-and-warn behavior is robust — one bad file doesn't break 3,519 others.

### `jq` queries are powerful
Built a burndown query: `find --property type=iteration --property status=in-progress --fields tasks --jq '.results | map({file, open: ..., total: ...})'` — worked perfectly.

### `links fix` with `--ignore-target` is useful
MDN has 48,979 broken links (mostly site-absolute). `links fix --ignore-target "/en-US/docs"` correctly filtered to just 212 unfixable non-MDN links and 0 fixable ones. Clean and fast (1.55s for 14K files).

### Performance remains excellent

| Command | Own KB (246) | MDN (14,245) | MDN (indexed) | GH Docs (3,520) |
|---|---|---|---|---|
| `find --limit 1` | 29ms | — | 687ms | — |
| BM25 search | 79ms | 3.8s | 657ms | 944ms |
| `summary` | 30ms | 1.07s | — | 381ms |
| `property filter` | 19ms | — | 672ms | 149ms |

Own KB is blazing fast (19-79ms). MDN indexed queries are consistent ~0.67s (index load dominated). GitHub Docs is fast at 149ms-944ms.

## Suggested Priorities

1. **BUG-2**: Add `--dry-run` to `properties rename` and `tags rename` — safety gap for bulk mutations
2. **BUG-1**: `mv --dry-run` should preview link rewrites — currently misleading (shows 0 changes)
3. **BUG-1 (prior)**: `--fields outline` alias for `sections` — easy UX win
4. **BUG-2 (prior)**: `--stemmer` ISO 639-1 codes — easy UX win
