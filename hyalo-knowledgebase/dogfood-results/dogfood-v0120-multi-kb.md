---
title: "Dogfood v0.12.0 — Multi-KB Session (MDN, GitHub Docs, Own KB)"
type: research
date: 2026-04-14
status: active
tags:
  - dogfooding
  - bm25
  - lint
  - types
  - schema
  - views
  - fts
  - mutations
---

# Dogfood v0.12.0 — Multi-KB Session

Exhaustive dogfood across three knowledgebases:
- **MDN Web Docs** (14,245 files) — large, external, mixed-case links
- **GitHub Docs** (3,520 files) — mid-size, complex frontmatter (nested YAML objects)
- **Own KB** (234 files) — small, well-structured, with schemas/views already defined

## Bugs

### BUG-1: `types set` writes to CWD's `.hyalo.toml`, not `--dir` target's (HIGH)

`hyalo types set mdn-doc --required title,slug,page-type --dir ../mdn/files/en-us/` writes to
`~/devel/hyalo/.hyalo.toml` (the working directory), not near the `--dir` target.
Then `hyalo types show mdn-doc --dir ../mdn/files/en-us/` fails with "type not found" because it reads from a different config path.

Same issue affects `views set` and `views list`. This was reported in the v0.11.0 dogfood as Bug 3 in iter-108, and the iter-108 fix only partially addressed it — types/views still use CWD config.

**Impact**: Impossible to manage types/views for external KBs when CWD has its own `.hyalo.toml`.

### BUG-2: `types set` reorders all TOML sections alphabetically (MEDIUM)

Running `types set` to add a single type causes the entire `.hyalo.toml` to be re-serialized in alphabetical section order. Also changes escape styles (e.g., `"^iter-\\d+[a-z]*/"` becomes `'^iter-\d+[a-z]*/'`). Creates 133-line diffs for a 1-line addition.

**Impact**: Version control diffs become useless. Any CI check for config changes would be noisy.

### BUG-3: `find "and"` / `find "or"` silently return 0 results (MEDIUM)

The words "and"/"or" are consumed as boolean operators, leaving an empty query. No error or warning is shown. A user searching for the word "and" would be confused.

**Suggestion**: Warn when the query consists entirely of boolean operators.

### BUG-4: `task toggle --all` misses deeply indented checkboxes (MEDIUM)

On GitHub Docs, a file with checkboxes at 16-space indentation gets "no tasks found" from `--all`, but `--line 76` toggles the same checkbox successfully. The `--all` scanner uses a stricter regex than `--line`.

### BUG-5: `create-index --index=PATH` ignores custom path (LOW)

`hyalo create-index --dir ../mdn/files/en-us/ --index=/tmp/mdn-hyalo-index` ignored the custom path and wrote to `../mdn/files/en-us/.hyalo-index` instead.

### BUG-6: `mv --dry-run` rewrites absolute URL-style links to broken relative paths (LOW)

On MDN, moving `games/anatomy/index.md` to `games/anatomy-renamed/index.md` would rewrite `/en-US/docs/Web/API/Web_Workers_API/Using_web_workers` to `../../en-US/docs/Web/API/Web_Workers_API/Using_web_workers`. Absolute URL-style links (used for internal routing) should be left alone.

### ~~BUG-7: FTS returns false positives for non-existent topics~~ — NOT A BUG

Investigation showed the matched files **do** contain the search terms. "Performance Benchmarks: hyalo v0.4.1" contains `deploy.*kubernetes` in benchmark command examples. The dogfood report itself mentions "kubernetes". Both matches are correct. BM25 ranking is working as intended.

### BUG-8: `remove --tag` rejects malformed comma-tags (LOW)

`hyalo remove file.md --tag "cli,ux"` fails with "invalid character ',' in tag name". This means malformed comma-tags (which lint doesn't catch either) can't be removed via the CLI at all. Need to edit files manually.

Similarly, `hyalo append file.md --property tags=` (empty value) silently adds `- ""` to the tags list instead of erroring.

## UX Issues

### ~~UX-1: No `--optional` flag for `types set`~~ — NOT AN ISSUE

Fields are optional by default. `--required` marks the mandatory subset; everything else in `properties` is implicitly optional. Use `--property-type` and `--property-values` to declare constraints on optional fields. CLAUDE.md reference was fixed.

### UX-2: No `--type` filter for `lint` (LOW)

`hyalo lint --type iteration` fails — must use `--glob "iterations/*.md"` instead. Lint already supports `--glob` and `--file` but not the same `--property`/`--tag` filters as `find`. A `--type` shortcut that expands to `--glob <filename-template>` would be convenient.

### UX-3: Lint doesn't detect comma-joined tags (LOW)

Own KB has 10 malformed tags like `"cli,ux"` and `"index,bug"` that should be separate YAML list items. `hyalo find --tag "cli,ux"` rejects these with "invalid character ','". Lint should catch this.

### UX-4: `task toggle` lacks `--dry-run` (LOW)

All other mutation commands have `--dry-run`. Task toggle doesn't.

### UX-5: Link resolution is case-sensitive (MDN-specific)

MDN links use mixed case (`/en-US/docs/Web/JavaScript/...`) but filesystem paths are lowercase. Even with `--site-prefix`, links are marked as unresolved. Would need a `--case-insensitive` flag or `.hyalo.toml` option.

### UX-6: Repeated frontmatter warning on every command (LOW)

GitHub Docs' `code-security/concepts/index.md` has unclosed frontmatter and triggers a warning on every hyalo command. Would benefit from an ignore list in `.hyalo.toml`.

## What Worked Great

### BM25 Full-Text Search — Excellent
- Ranking is spot-on: `find "snapshot index"` → iter-47 (score 5.93), `find "BM25 ranking"` → iter-101 (score 11.41), `find "CSS grid layout"` → CSS grid module page (score 16.47)
- Boolean operators (AND, OR, NOT) work intuitively
- Phrase search with quotes works well
- Low-score warning for stopword-heavy queries is helpful
- Combined FTS + structured search (`find "Promise" --property page-type=web-api-instance-method`) works perfectly

### Performance at Scale
- 14,245 MDN files: summary 0.74s, FTS 3.15s (no index), 0.36s (with index) — 8.7x speedup
- 3,520 GitHub Docs: summary 0.2s, structured search 0.15s, FTS 0.9s
- Index creation: 2.5s for 14,245 files

### Structured Search — Very Powerful
- Multiple `--property` filters combine correctly
- Regex on JSON-as-string values works (`--property 'versions~=ghes'` on GH Docs)
- Negated filters (`--property '!browser-compat'`) work
- Date range filters (`>=`, `<=`) work
- Tag filters combine with property filters and FTS

### Views — Great Feature
- 7 views defined in own KB, all work via `find --view <name>`
- `find --view stale-in-progress` shows actionable results with task counts

### Mutation Operations — Solid
- `set`, `remove`, `append` all work correctly with clean round-trips
- Multi-property operations in single command work
- `--glob` for multi-file mutations with `--dry-run` works well

### Task Toggle — Well-Behaved
- Round-trip toggling works perfectly
- Edge cases (no tasks, non-existent section, invalid line) all give clear errors

### General UX
- Hint system provides excellent discoverability
- `--orphan --dead-end` mutual exclusivity warning is great
- `--count` and `--jq` bypass limits correctly
- `--format text` is consistently useful and compact
- Error messages are clear and actionable across all commands

## CLAUDE.md Doc Bugs

1. References `--optional` flag for `types set` — doesn't exist
2. References `hyalo lint --type iteration` — doesn't exist (use `--glob`)

## Data Quality Issues (Own KB)

- ~~10 tags contain commas (e.g., `"cli,ux"`, `"cli,find,ux"`, `"index,bug"`)~~ — **FIXED** during this session (13 files in `backlog/done/`)
- `priority` property has mixed types: 6 files use number, 84 use text

## Previously Reported — Still Open

- BUG-1 (`--dir` config lookup) was reported in v0.11.0 dogfood, iter-108 partially fixed it
- BUG-6 (absolute link rewriting) was reported in v0.4.0 dogfood

**Superseded by [[iterations/iteration-118-split-index-flag]]:** `--index=PATH` is now `--index-file=PATH`; bare `--index` is a boolean flag.

## Suggested Iteration Priority

1. **HIGH**: Fix `--dir` config lookup for `types set` / `views set` (BUG-1) — blocks external KB management
2. **MEDIUM**: Fix TOML section reordering in `types set` (BUG-2) — use toml_edit for order-preserving writes
3. **MEDIUM**: Warn on bare boolean operator queries (BUG-3)
4. **MEDIUM**: Fix `task toggle --all` indentation regex (BUG-4)
5. **LOW**: Fix `create-index --index=PATH` (BUG-5)
6. **LOW**: Add `lint --warn-comma-tags`, `task toggle --dry-run`, `lint --type` shortcut
7. **LOW**: Fix `remove --tag` / `append --property tags=` edge cases for malformed tags (BUG-8)
8. **DOCS**: ~~Fix CLAUDE.md references to `--optional` and `lint --type`~~ — `--optional` fixed during session
