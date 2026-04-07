---
title: Token Consumption Analysis for LLM Users
type: research
date: 2026-04-07
tags:
  - research
  - performance
  - llm
  - token-optimization
status: completed
---

# Token Consumption Analysis for LLM Users

Research into how much output hyalo produces across real-world documentation repos, where tokens are wasted, and what optimizations would have the highest impact.

## Test Repos

| Repo | Files | Description |
|---|---|---|
| github/docs | 7,352 | GitHub documentation |
| vscode-docs | 760 | VS Code documentation |
| mdn/content | 14,264 | MDN Web Docs |

## Measurements

### Summary Command

| Repo | JSON | Text | ~Tokens (JSON) |
|---|---|---|---|
| docs | 4.4M chars | 311K chars | ~1.1M |
| mdn | 12.1M chars | — | ~3.0M |
| hyalo KB (small) | — | 7.9K chars | ~2K |

**Breakdown of JSON summary (docs repo):**

| Section | Size | Notes |
|---|---|---|
| links (broken_links array) | 3,252K chars | **93% of total** — 15,110 broken links listed in full |
| orphans | 255K chars | 3,948 orphan file paths |
| files (by_directory) | 71K chars | directory tree with counts |
| properties | 2.4K chars | reasonable |
| recent_files | 1.6K chars | reasonable |
| tags, tasks, status | <1K chars | reasonable |

**Key finding:** The `broken_links` array in JSON summary is the #1 token hog, consuming 93% of total output. An LLM almost never needs all 15,110 broken link paths as a first step.

### Find Command (No Filter)

| Repo | Text output | ~Tokens |
|---|---|---|
| docs | 5.6M chars | ~1.4M |
| vscode-docs | 1.0M chars | ~263K |
| mdn | 10.5M chars | ~2.6M |

Running `hyalo find` with no filter on a large repo dumps the entire directory into context — unusable for LLMs.

### Find Command (Body Search)

Search for "flexbox" in mdn (144 matches):

| Variant | Output | ~Tokens | Reduction |
|---|---|---|---|
| Default (all fields) | 299K chars | ~75K | baseline |
| `--limit 5` | 7.1K chars | ~1.8K | **97%** |
| `--fields title` + `--limit 5` | ~3.5K chars | ~900 | **99%** |
| `--count` | 3 chars | 1 | **100%** |
| `--jq '.results[].file'` + `--limit 5` | ~250 chars | ~63 | **100%** |

### Per-Result Overhead

Each search result in text format includes by default:
- File path + modified timestamp
- All frontmatter properties (key: value pairs)
- All section headings
- Full matching lines (entire paragraph, can be 500+ chars)
- All outbound links

For a "which files mention X?" query, the LLM only needs: file path, title, match count.

### Hints Overhead

Hints add ~600 chars per query — negligible compared to result data, and often useful for guiding the next step.

## Where Tokens Are Wasted

1. **`summary` broken_links array** — 3.2M chars for docs repo. Should be opt-in, not default.
2. **Unbounded `find`** — No default limit means a typo or broad search dumps megabytes.
3. **Full-length match lines** — MDN paragraphs can be 500+ chars; only ~120 chars around the match term is needed for relevance.
4. **Links in search results** — 8 link lines per file when the LLM was searching body text, not link structure.
5. **Properties/sections in search results** — Useful for drilling in, but wasteful for initial triage of "which files match?"

## Optimization Ideas

### A. `--brief` mode (or `--format brief`)

One line per file: `path | title | match_count`. For 144 results: ~5K chars instead of 299K.

- **Savings:** 95%+ on search results
- **Effort:** Small
- **Use case:** Initial triage — "which files mention X?" then drill into specific ones

### B. Cap summary output for large repos

Default summary should be a dashboard: file count, top-2-level directory tree, property/tag/status counts, task counts, recent 5 files. Cap at ~2-3K chars.

Detailed broken links / orphans should be opt-in (`summary --broken-links`, `summary --orphans`).

- **Savings:** 95%+ on large repos
- **Effort:** Medium

### C. Match snippet truncation (`--snippet-width N`)

Truncate body match lines to N chars centered around the match term (default ~120 chars with `...` ellipsis).

- **Savings:** 50-70% on body search results
- **Effort:** Small

### D. `default_limit` in `.hyalo.toml`

A config setting like `default_limit = 20` that prevents unbounded output. Override with explicit `--limit 0` or `--limit 999`.

- **Savings:** Prevents catastrophic unbounded output
- **Effort:** Small

### E. `--format llm` preset

Combines: brief output, auto `--limit 20`, snippet truncation. Single flag: "I'm an LLM, be terse."

Could be set as default format in `.hyalo.toml`:
```toml
format = "llm"
```

- **Savings:** Combines A+C+D in one flag
- **Effort:** Medium

### F. `--fields none` for file-paths-only output

Currently `--fields ""` works but is undocumented. A clean `--fields none` alias would make the "list matching paths" workflow obvious.

- **Savings:** 80% for "list files" queries
- **Effort:** Tiny

## Recommended Priority

| Priority | Idea | Token savings | Effort |
|---|---|---|---|
| 1 | **B. Cap summary** | 95%+ on large repos | Medium |
| 2 | **A. `--brief` mode** | 95% on search results | Small |
| 3 | **D. `default_limit` config** | Prevents unbounded output | Small |
| 4 | **C. Snippet truncation** | 50-70% on body search | Small |
| 5 | **E. `--format llm`** | Combines A+C+D | Medium |
| 6 | **F. `--fields none`** | 80% for path listings | Tiny |

## Single Highest-Impact Change

**`--format llm`** (or `--brief`) that outputs one line per file with `path | title | match_count` and auto-limits to 20 results with a `showing 20 of N` footer. This one flag would turn a 300K-char search into a 2K-char search for the 90% case where the LLM just needs to pick which file to drill into.
