---
title: "Iteration 36 — Advanced Filtering"
type: iteration
date: 2026-03-23
tags: [iteration, filtering, cli, ux]
status: planned
branch: iter-36/advanced-filtering
---

# Iteration 36 — Advanced Filtering

## Goal

Expand `hyalo find` filtering to cover the common queries that currently require multiple round-trips or workarounds. More expressive filters = fewer tool calls = fewer tokens for Claude Code.

## Backlog items

- [[backlog/absence-filter]] (medium)
- [[backlog/glob-negation]] (medium)
- [[backlog/section-filter-substring-matching]] (medium)
- [[backlog/property-value-search]] (low)
- [[date-aware-comparison]] (low)

## Tasks

### Property absence filter
- [ ] `--property !K` or `--no-property K` filters for files missing property K
- [ ] Works in combination with other filters (AND semantics)
- [ ] Help text documents the syntax
- [ ] E2e tests cover absence filter

### Glob negation / exclude patterns
- [ ] `--glob '!pattern'` or `--exclude` flag excludes matching files
- [ ] Works in combination with positive globs (include + exclude)
- [ ] Help text documents the syntax
- [ ] E2e tests cover negation globs

### Section filter substring matching
- [ ] `--section` supports substring/prefix matching (not just exact)
- [ ] Exact match still works (backwards compatible)
- [ ] E2e tests cover substring section matching

### Property value substring/regex search
- [ ] `--property 'K~=pattern'` searches within text property values
- [ ] Works with list properties (matches if any element matches)
- [ ] Help text documents the syntax
- [ ] E2e tests cover value search

### Date-aware comparison
- [ ] Date property comparisons produce chronologically correct results
- [ ] Works with ISO 8601 date format (YYYY-MM-DD)
- [ ] Non-date values fall back to string comparison
- [ ] E2e tests cover date comparisons (`--property 'date>=2026-01-01'`)

## Acceptance Criteria

- [ ] All five filter enhancements work individually and in combination
- [ ] Help text is updated for all new syntax
- [ ] All quality gates pass (fmt, clippy, tests)
