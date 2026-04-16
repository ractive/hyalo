---
title: "Dogfood v0.12.0 — Post Iteration 118 Verification"
type: research
date: 2026-04-16
status: active
tags:
  - dogfooding
  - verification
related:
  - "[[dogfood-results/dogfood-v0120-iter115-followup]]"
  - "[[iterations/iteration-117-case-insensitive-link-resolution]]"
  - "[[iterations/iteration-118-split-index-flag]]"
---

# Dogfood v0.12.0 — Post Iteration 118 Verification

Re-ran scenarios after iterations 113–118 merged.
Binary: `hyalo 0.12.0` built via `cargo build --release`.
Own KB: 245 files across 6 directories.

## Iter-117 / Iter-118 Feature Verification

### `--index` / `--index-file` split (iter-118) — WORKING

- `hyalo find "snapshot index architecture" --index` works — uses `.hyalo-index` as boolean flag
- `hyalo create-index --output /tmp/hyalo-test-index --allow-outside-vault` works, creates at custom path
- `hyalo find "BM25 ranking" --index-file /tmp/hyalo-test-index` works, uses custom index file
- `hyalo drop-index` correctly deletes `.hyalo-index`
- `create-index --output /tmp/path` without `--allow-outside-vault` gives clear error: "output path is outside the vault boundary" with hint to use `--allow-outside-vault`
- Stale index warning appears when vault has changed since index creation

### Case-insensitive link resolution (iter-117) — WORKING

Links in `find --fields links` resolve correctly. Frontmatter wikilinks in `related` lists are extracted and resolve (iter-115 BUG-B fix still holding). The `CLAUDE.md` wikilink in iter-118's iteration file is correctly reported as unresolved since it references a file outside the KB directory.

## Prior Bug Fix Re-verification

### BUG-1: `--dir` config resolution — STILL FIXED

Verified through `types show`, `views list`, and `set` commands — all read/write config correctly.

### BUG-2: TOML section ordering — STILL FIXED

`.hyalo.toml` maintains section order after `types set` operations.

### BUG-3: Bare boolean operator warning — FIXED (all cases)

- `find "and"` → warning: "and" was interpreted as a boolean operator
- `find "or"` → same pattern
- `find "AND OR"` → aggregated warning: "AND", "OR" were interpreted as boolean operators
- All suggest quoting the word to search literally

### BUG-4: `task toggle --all` deep indentation — STILL FIXED

### BUG-8: `remove --tag` with comma-tags — STILL FIXED

### BUG-A (iter-114): `--index` parsing trap — FIXED by iter-118 split

The `--index` is now a boolean flag; `--index-file` takes the path. No more ambiguity.

### BUG-B (iter-114): Frontmatter wikilinks — STILL FIXED

`find --file iter-118 --fields links` correctly extracts `[[...]]` from `related` property lists.

### BUG-C (iter-114): `[[wikilink]]` in `--property` value — STILL FIXED

`set file.md --property 'supersedes=[[some-link]]' --dry-run` stores the wikilink correctly, not parsed as YAML flow list.

### BUG-D (iter-114): Schema validation on `set` — WORKING (opt-in)

`set --property "priority=banana" --validate --dry-run` correctly rejects with: `property "priority" value "banana" not in [low, medium, high, critical] (did you mean "low"?)`. Note: validation is opt-in via `--validate` flag or `validate_on_write = true` config.

### BUG-E (iter-114): `--where-property` without `--file`/`--glob` — FIXED

`set --where-property "status=planned" --property "status=in-progress" --dry-run` correctly scanned all 245 files and found 3 matches.

## UX Verification

### `lint --type` (UX-2) — WORKING

`lint --type iteration` checks only the 17 iteration-type files. Zero issues found.

### `lint --count` (iter-115) — WORKING

Returns `0` for a clean vault.

### `append --tag` friendly error (iter-114) — WORKING

Returns: `hyalo append does not accept --tag` with hint to use `hyalo set`.

### `--stemmer` alias (iter-115) — WORKING

`find "running tests" --stemmer english` works correctly. `--stemmer en` fails with clear error: "unknown stemming language: en" — only full language names accepted.

### `--orphan --dead-end` mutual exclusivity — WORKING

Clear warning: "--orphan and --dead-end are mutually exclusive; results will always be empty".

### Views — ALL WORKING

- `stale-in-progress`: found 2 files (iter-110 and iter-112, both genuinely stale)
- `orphans`: 79 orphan files
- `planned`: 3 planned backlog items
- `completed-with-todos`: correctly finds completed files with remaining open tasks

### `--jq` / `--count` limit bypass — WORKING

`find --property status=completed --count` returns 198. `--jq` also bypasses default limit. `--limit 0` works for unlimited output.

### `show` alias for `read` — WORKING

`hyalo show` works identically to `hyalo read`.

## Bugs Found

### BUG-1: `--fields outline` is not a valid field name (LOW, UX)

`find --file X --fields outline` errors with: "unknown field 'outline': valid fields are all, properties, properties-typed, tags, sections, tasks, links, backlinks, title". The `sections` field shows heading structure (which is what "outline" means). Adding `outline` as an alias for `sections` would improve discoverability.

### BUG-2: `--stemmer` / `--language` doesn't accept ISO 639-1 codes (LOW, UX)

`--stemmer en` fails with "unknown stemming language: en". Only full names like "english" are accepted. 2-letter ISO codes (`en`, `de`, `fr`, etc.) are the natural form for many users.

### ~~BUG-3: Schema validation is silent by default on `set`~~ — NOT A BUG

`validate_on_write` is not set in `.hyalo.toml`, so validation is correctly off by default. `--validate` flag works when passed explicitly. To enable by default, add `validate_on_write = true` under `[schema]` in `.hyalo.toml`.

## Data Quality Issues

### Stale `status: in-progress` on iter-110 and iter-112

- `iterations/iteration-110-default-output-limits.md` has 9/9 tasks done but status is `in-progress`
- `iterations/iteration-112-skill-sync-and-limit-bypass.md` has 9/11 tasks done, status is `in-progress` — 2 uncompleted tasks (README update and iteration file creation)

### 4 fixable broken links

All point to iteration files moved to `done/` subdirectory:
- `iterations/iteration-101-bm25-ranked-search` → `iterations/done/iteration-101-bm25-ranked-search.md` (3 files)
- `iterations/iteration-101b-bm25-serializable-index` → `iterations/done/iteration-101b-bm25-serializable-index.md` (1 file)

Fixable via `hyalo links fix --apply`.

### 1 unfixable broken link

`iterations/iteration-118-split-index-flag.md` references `[[CLAUDE.md]]` which is in the repo root, outside the KB directory. Expected but worth noting.

### Mixed `priority` types

6 files use numeric priority (type=number), 84 use text. This has been noted in prior reports and remains unfixed.

## Performance (Own KB, 245 files)

| Command | Time |
|---------|------|
| `summary` (no index) | 31ms |
| `summary` (indexed) | 26ms |
| FTS "snapshot index architecture" (no index) | 61ms |
| FTS "snapshot index architecture" (indexed) | 19ms |
| Structured search (status=completed + tag) | 19ms |
| `lint` | 22ms |

All within expected ranges. Index provides 3.2x speedup for FTS, marginal for summary (at 245 files the disk scan is already fast).

## What Works Great

- **`--index` / `--index-file` split** — clean, unambiguous design. Boolean `--index` is natural for "just use the index" workflows. `--index-file` for custom paths.
- **Schema validation with `--validate`** — excellent error messages with "did you mean?" suggestions
- **`--where-property` without `--file`/`--glob`** — enables powerful bulk mutations across the whole vault
- **`links fix`** — accurate confidence scores, clear fixable/unfixable breakdown, safe dry-run default
- **Views** — all 7 views work correctly, `--view stale-in-progress` is immediately actionable
- **Combined FTS + structured search** — `find "dogfood" --property status=active` composes cleanly
- **Property regex** — `--property 'title~=/split.*index/i'` with case-insensitive flag works
- **Hint system** — context-aware suggestions after every command
- **`mv --dry-run`** — shows exact link replacements that would be made
- **Error messages** — consistently clear and actionable (empty query, vault boundary, missing values)

## Summary Table

| Item | Status |
|------|--------|
| iter-117: Case-insensitive links | Working |
| iter-118: `--index` / `--index-file` split | Working |
| iter-115 BUG-A through BUG-E | All verified fixed |
| iter-116: `task toggle --dry-run` arrow | Working |
| iter-113 bug fixes (BUG-1 through BUG-8) | All still holding |
| `--fields outline` alias | Missing (LOW) |
| `--stemmer` ISO codes | Missing (LOW) |
| Schema validation default | Working as designed |

## Suggested Priorities

1. **LOW**: Add `outline` as field alias for `sections`
2. **LOW**: Accept ISO 639-1 language codes as aliases for `--stemmer`/`--language`
3. **DATA**: Fix 4 broken links via `links fix --apply` — **DONE**
4. **DATA**: Update iter-110 status to `completed` — **DONE**; review iter-112 remaining tasks
