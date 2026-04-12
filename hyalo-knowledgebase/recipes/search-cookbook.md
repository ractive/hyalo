---
title: "Search Cookbook — BM25 & Regex Recipes"
type: reference
date: 2026-04-12
tags:
  - reference
  - search
  - bm25
  - cookbook
---

# Search Cookbook

Practical recipes for `hyalo find`. Two search modes: **BM25** (ranked full-text with stemming) and **Regex** (pattern matching, unranked).

## BM25 Full-Text Search

### Basic search

```bash
# Find documents about "authentication"
# (also matches "authenticate", "authenticating" via stemming)
hyalo find "authentication"

# Find documents mentioning both "rust" and "performance"
# (space = AND, both required)
hyalo find "rust performance"
```

### OR — match any term

```bash
# Find documents about either language
hyalo find "rust OR golang"

# Find docs about any error type
hyalo find "timeout OR deadlock OR panic"
```

### Negation — exclude terms

```bash
# Rust docs that don't mention javascript
hyalo find "rust -javascript"

# Search for "deploy" but exclude anything about staging
hyalo find "deploy -staging"

# Combine OR with negation
hyalo find "rust OR golang -deprecated"
```

### Phrase search — exact consecutive match

```bash
# Match the exact phrase (after stemming)
hyalo find '"error handling"'

# Phrase + negation
hyalo find '"error handling" -panic'

# Phrase with other terms (AND)
hyalo find '"dependency injection" testing'
```

### Combine with filters

```bash
# BM25 search scoped to a tag
hyalo find "authentication" --tag security

# Search within in-progress iterations
hyalo find "refactor" --property status=in-progress --tag iteration

# Search within a specific section
hyalo find "TODO" --section "Tasks"

# Search only in specific files
hyalo find "bug" --glob "iterations/*.md"

# Limit and sort
hyalo find "performance" --limit 5
hyalo find "testing" --sort modified --reverse --limit 10
```

### Language-specific stemming

```bash
# French stemming ("coureur" matches "courir", "course", etc.)
hyalo find "coureur" --language french

# German stemming
hyalo find "Veränderung" --language german
```

## Regex Search

### Basic patterns

```bash
# Find TODO/FIXME/HACK markers
hyalo find -e "TODO|FIXME|HACK"

# Find function-like patterns
hyalo find -e "fn\s+\w+_test"

# Case-sensitive regex (default is case-insensitive)
hyalo find -e "(?-i)README"
```

### Combine regex with filters

```bash
# Regex in tagged files
hyalo find -e "TODO|FIXME" --tag iteration --property status=in-progress

# Regex scoped to a section
hyalo find -e "^\s*-\s*\[[ x]\]" --section "Tasks"
```

## When to use which

| Need | Use |
|---|---|
| "Find docs about X" (conceptual search) | `hyalo find "X"` (BM25) |
| Exact string or pattern match | `hyalo find -e "pattern"` (regex) |
| Relevance-ranked results | BM25 (scored and sorted) |
| Line-level matches with context | Regex (returns match lines) |
| Stemmed matching (run → running) | BM25 |
| Multiple alternative terms | `hyalo find "X OR Y"` (BM25) |
| Exclude a concept | `hyalo find "X -Y"` (BM25) |
| Complex boolean | `hyalo find '"exact phrase" term -excluded'` (BM25) |

## Tips

- **Implicit AND is the default.** `hyalo find "rust performance"` requires *both* words. This matches Google/GitHub behavior.
- **OR must be explicit.** Write `rust OR golang`, not just `rust golang` (which means AND).
- **Stemming applies everywhere.** `-running` also excludes "run", "runner". Phrase `"error handling"` matches "errors handled".
- **Combine BM25 with filters freely.** Metadata filters (property, tag, section) and BM25 scoring are logically combined — the result is documents matching all criteria, ranked by relevance.
- **Use `--index` for large vaults.** On 500+ files, `create-index` makes BM25 queries 6x faster by persisting the inverted index.
- **Score field.** BM25 results include a `score` field — higher is more relevant. Use `--jq '.results | sort_by(-.score) | .[0:3]'` for top 3.
