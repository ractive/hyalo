---
title: "Dogfooding v0.10.0 — Post-BM25, Views, Summary Redesign"
date: 2026-04-12
type: research
status: active
tags:
  - dogfooding
  - performance
  - bm25
  - views
  - summary
---

# Dogfooding v0.10.0 — Post-BM25, Views, Summary Redesign

Tested on hyalo-knowledgebase (221 files, 102 iterations, 84 backlog items, 26 research docs).

## Overall Verdict

Excellent. No bugs found. All features work correctly. Performance is outstanding. Both previously reported v0.6.0 bugs (title regex on derived titles, text-format backlinks) are confirmed fixed.

## New Feature Assessment

### Summary Redesign (iter-105)

**Verdict: Great.** The compact text output is a massive improvement — fits in ~12 lines and gives a complete vault overview at a glance. Key stats: files, directories (with counts), properties, tags, tasks (done/total), links (total + broken), orphans, dead-ends, status distribution, recent files. Hints point to natural drill-down commands.

**Minor nits:**
- The "Directories" line lists top-level dirs by count — useful but would be nice to see a percentage or bar
- "Status" line lists all values sorted alphabetically — might be more useful sorted by count (completed: 184 first, not "active: 3" first)

### BM25 Full-Text Search (iter-101, 101b, 103)

**Verdict: Excellent.** Boolean operators work intuitively:

| Query | Behavior | Works? |
|---|---|---|
| `search ranking` | Implicit AND | Yes |
| `"broken links"` | Phrase match | Yes |
| `search -vector` | Negation | Yes |
| `obsidian OR zettelkasten` | Explicit OR | Yes |
| `dogfood` + `--tag iteration` | BM25 + metadata filter | Yes |
| `dogfood` + `--view planned` | BM25 + saved view | Yes |
| Empty `""` | Error with helpful message | Yes |
| Stopwords only (`the and or`) | Returns 205 matches, very low scores | See UX-1 |
| Version number `v0.5.0` | Works, finds version mentions | Yes |
| Stemming (`running` → `run`) | Correctly stems | Yes |

Score ordering is sensible — exact matches and high-density documents rank first.

### Views (iter-94, 96)

**Verdict: Solid.** CRUD works: `views set`, `views list`, `views remove`. Composing `--view` with additional CLI flags (like `--sort`, `--reverse`) works correctly. Unknown view gives a clear error with tip. The pre-configured views (planned, open-tasks, stale-in-progress, etc.) are genuinely useful.

**Gap:** Views cannot save a BM25 search pattern. The `views set` command takes all `find` filter flags but not the positional `PATTERN` argument. This means you can save `--property status=planned --tag iteration` as a view, but not `"search query" --tag iteration`.

### Orphan / Dead-End Filters (iter-105)

**Verdict: Works correctly.** 75 orphans, 72 dead-ends. These are mutually exclusive by definition (orphans have no links in/out; dead-ends have inbound but no outbound), so `--orphan --dead-end` returns 0 results. Not a bug, but could be confusing — see UX-2.

## UX Issues

### UX-1 (LOW): Stopword-only queries return noisy results

`hyalo find "the and or"` returns 205/221 files with scores like 0.16. All search terms are common stopwords, so the results are essentially noise. Consider:
- Warning when all tokens are stopwords
- Or returning "No meaningful search terms after stemming" error

### UX-2 (LOW): `--orphan --dead-end` silent empty result

Combining `--orphan` and `--dead-end` returns nothing because they're mutually exclusive by definition. A hint or warning ("orphan and dead-end are disjoint sets — did you mean OR?") would help users who expect union semantics.

### UX-3 (LOW): Views can't store BM25 patterns

`views set` accepts all filter flags but not the positional search pattern. A `--pattern` or positional arg would complete the feature:
```bash
hyalo views set perf-iterations "performance" --tag iteration
```

### UX-4 (LOW): Summary status sorted alphabetically, not by count

The Status line in `summary` shows `active (3), completed (184), ...` — alphabetical. Sorting by count (completed first) would make the most common statuses scannable faster.

## Performance (hyalo-knowledgebase, 221 files, no index)

| Command | Time | vs v0.6.0 |
|---|---|---|
| `find --limit 1` | 17ms | **12x faster** (was 200ms) |
| `find "dogfood" --limit 1` (BM25) | 51ms | new feature |
| `summary` | 18ms | **52x faster** (was 940ms) |
| `properties` | 15ms | **59x faster** (was 880ms) |

With index:

| Command | Time |
|---|---|
| `find "dogfood" --index --limit 1` | 11ms |

**Massive performance improvement** since v0.6.0 — likely due to the unified vault index and scan optimizations. The knowledgebase is small (221 files), but the absolute times are excellent.

## What Works Great

1. **Hints system** — every command outputs contextual, copy-pasteable drill-down commands with descriptions. This is a killer LLM-agent feature.
2. **BM25 + metadata filter composition** — `find "query" --tag X --property Y=Z` is powerful and natural.
3. **Error messages** — helpful, with tips pointing to the right command.
4. **`--count`** — quick way to get totals without parsing JSON.
5. **`links fix`** dry-run with apply hint — safe default.
6. **Task read with section scoping** — `task read FILE --section "Quality Gates"` is exactly what an LLM agent needs.

## Previously Reported Bugs — Status

| Bug | Status |
|---|---|
| BUG 1: `title~=` doesn't match derived titles | **FIXED** |
| BUG 2: Text format drops backlinks | **FIXED** |
| `links fix` false positives for short paths | Still present (5 broken links all map to `iteration-plan` → `iteration-02-links.md`), but reasonable given the fuzzy matching |

## Improvement Ideas (Informed by qmd Gap Analysis)

### High Priority

1. **MCP Server** — Expose hyalo's structured queries as MCP tools. hyalo's metadata filtering + BM25 search + task management would be more powerful than qmd's search-only MCP. Tools: `query` (find), `get` (read), `mutate` (set/remove/append), `status` (summary).

2. **`--explain` flag for BM25** — Show per-result score breakdown (term frequencies, IDF weights). qmd has this and it's useful for tuning queries.

### Medium Priority

3. **Views with BM25 patterns** — Complete the views feature by allowing saved search patterns, not just filter flags.

4. **Summary status sort by count** — Small change, big readability win.

5. **`find --no-links` / `--has-links`** — Complement `--orphan` and `--dead-end` with simpler "has any links" / "has no outgoing links" filters. The current terminology is precise but requires understanding the orphan/dead-end definitions.

6. **`hyalo stats`** — A more detailed version of summary focused on vault health metrics: avg properties per file, property completeness %, tag distribution histogram, link density, task completion rate over time.

### Low Priority

7. **Stopword-only query warning** — Gentle UX improvement for BM25 edge case.

8. **`--orphan --dead-end` hint** — Warn when combining mutually exclusive filters.

9. **Non-markdown file support** — Even basic plaintext indexing for `.txt`, `.yml`, `.toml` files in the vault would be useful. qmd supports code files via tree-sitter, but even without AST parsing, body search over non-markdown would help.

10. **SDK / library crate** — Extract `hyalo-core` as a reusable Rust library for programmatic access. This would enable: custom CLI wrappers, editor plugins, web UIs, and the MCP server as a thin consumer.
