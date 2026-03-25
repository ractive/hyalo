---
title: "Iteration 36 — Advanced Filtering"
type: iteration
date: 2026-03-25
tags: [iteration, filtering, cli, ux]
status: planned
branch: iter-36/advanced-filtering
---

# Iteration 36 — Advanced Filtering

## Goal

Expand `hyalo find` filtering to cover the common queries that currently require multiple round-trips or workarounds. Unified operator syntax across `--property` and `--section`. More expressive filters = fewer tool calls = fewer tokens for Claude Code.

## Backlog items

- [[backlog/absence-filter]] (medium)
- [[backlog/glob-negation]] (medium)
- [[backlog/section-filter-substring-matching]] (medium)
- [[backlog/property-value-search]] (low — folded into `~=` operator)

## Design decisions

### Property filter operator syntax

```
--property K                  # existence (has property K)
--property '!K'               # absence (missing property K)
--property K=V                # equals
--property K!=V               # not equals
--property K>V  K>=V  K<V  K<=V  # comparison (string/numeric)
--property 'K~=pattern'       # regex match on value (unanchored)
--property 'K~=/pattern/flags'  # regex with flags (e.g. /i for case-insensitive)
```

`~=` is always unanchored regex (contains semantics). Bare `K~=bar` and `K~=/bar/` are equivalent. Use `^`/`$` anchors for exact match (`K~=/^bar$/`). Regex behavior matches `-e`/`--regexp` on body content.

For list properties, `~=` matches if any element matches.

### Section filter: substring by default

`--section` changes from exact (whole-string) to substring (contains) matching, case-insensitive. This fixes the pain point with headings that have date/counter suffixes.

```
--section "Tasks"              # substring: matches "Tasks", "Tasks [4/4]", "My Tasks"
--section "## Tasks"           # substring + level-pinned
--section "~=/DEC-03[12]/"     # regex (power users)
```

Existing queries return a superset (any exact match is also a substring match), so this is backwards compatible in practice.

### Glob negation: rg-style `!` prefix

```
--glob '!**/index.md'          # exclude all index.md files
--glob 'notes/*.md' --glob '!notes/draft-*'  # include + exclude
```

Follows ripgrep convention. Negation globs are AND'd: file must match all positive globs and not match any negation glob.

## Tasks

### Property absence filter
- [ ] Parse `!K` syntax in property filter (distinguish from `K!=V`)
- [ ] `PropertyFilter` variant for absence check
- [ ] Works in combination with other filters (AND semantics)
- [ ] Help text documents the `!K` syntax
- [ ] E2e tests cover absence filter

### Property value regex (`~=`)
- [ ] Parse `K~=pattern` and `K~=/pattern/flags` syntax
- [ ] Unanchored regex match on property value (string representation)
- [ ] For list properties, match if any element matches
- [ ] Regex size limit (reuse existing defense-in-depth pattern from content search)
- [ ] Help text documents the `~=` syntax with examples
- [ ] E2e tests cover bare pattern, `/pattern/`, `/pattern/i`, list properties

### Section filter substring matching
- [ ] Change `SectionFilter` from exact to substring (contains) matching
- [ ] Level pinning (`## Foo`) still works with substring
- [ ] Add `~=/regex/` support for section headings
- [ ] Update error hint (section not found) to reflect new matching
- [ ] E2e tests: substring match, regex match, level-pinned substring

### Glob negation
- [ ] Parse `!` prefix in glob patterns
- [ ] Negation globs exclude files from results
- [ ] Works with `--glob` on all commands (find, set, remove, append, properties, tags, summary)
- [ ] Works in combination with positive globs (repeatable)
- [ ] Help text documents negation syntax
- [ ] E2e tests cover negation, combined include+exclude

## Acceptance Criteria

- [ ] All four filter enhancements work individually and in combination
- [ ] Help text and README updated for all new syntax
- [ ] All quality gates pass (fmt, clippy, tests)

## Deferred

- **Date-aware comparison** — ISO dates already sort correctly as strings; non-ISO formats (MM/DD/YYYY) are rare. Deferred to a future iteration.
