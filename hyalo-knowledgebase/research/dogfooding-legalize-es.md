---
title: "Dogfooding: legalize-es (8,642 Spanish laws)"
type: research
date: 2026-03-28
tags:
  - dogfooding
  - performance
  - ux
status: completed
---

# Dogfooding: legalize-es (8,642 Spanish laws)

Tested hyalo v0.5.0 against [legalize-es](https://github.com/legalize-dev/legalize-es), a corpus of 8,642 Spanish law files (BOE consolidated legislation from 1835–2026). All files have identical YAML frontmatter schema (titulo, identificador, pais, rango, fecha_publicacion, ultima_actualizacion, estado, fuente). No tags, no wikilinks, no tasks. Single flat directory.

## Performance results

| Command | Without index | With index | Speedup |
|---|---|---|---|
| summary | 1.9s | 0.04s | 47x |
| find (property filter) | 2.3s | 0.02s | 85x |
| find (body search, "habeas corpus") | 3.0s | n/a | — |
| properties summary | 0.3s | — | — |
| backlinks (full scan) | 1.8s | — | — |
| bulk remove (8,642 files) | 2.5s | — | — |
| create-index | 2.4s | — | — |

Performance is excellent at this scale. The snapshot index provides 50–85x speedup on read-only operations.

## Bugs found

### set --property silently accepts filter syntax as property name (HIGH)

Running `hyalo set --property 'fecha_publicacion<=1900-01-01' --glob '*.md'` creates a literal property key `fecha_publicacion<` with value `1900-01-01` on ALL files. The `<=` is parsed as a key-value separator, not rejected.

This accidentally wrote a garbage property to all 8,642 files. The correct flag is `--where-property`, but `set` should reject comparison operators in `--property` and suggest `--where-property`.

### False-positive broken links from square brackets in legal text (MEDIUM)

`[[1]]` (footnote reference) and `[0,35 * kms.recorridos (n – 1)]` (mathematical formula) are parsed as wikilinks. Creates noise in `links fix` and `summary`. Consider heuristics: purely numeric or math-operator-containing "link targets" should be ignored.

## UX issues

### Summary orphan list overwhelms output on flat repos (MEDIUM)

With 8,640 orphans (expected for a linkless repo), summary output is dominated by the orphan list. Hints at the bottom are buried. Suggestion: cap orphan list in summary to ~10 files by default with a "(and N more)" note. Or add `--no-orphans` flag.

### --property vs --where-property is a foot-gun (MEDIUM)

`find` uses `--property` for filtering. `set` uses `--property` for mutation and `--where-property` for filtering. This asymmetry caught me — I instinctively used `--property` for filtering in `set` because that's how `find` works. At minimum, `set --property` should warn when it encounters comparison operators.

### properties rename lacks --dry-run (LOW)

`mv` has `--dry-run` but `properties rename` and `tags rename` don't. For bulk operations on large repos, a preview would be useful.

## What worked well

- Date comparisons handle 19th-century dates correctly
- Title regex (`--property 'titulo~=COVID'`) found 263 COVID laws instantly
- Body search for "habeas corpus" across 8,600 files in 3s
- `--jq` integration for extracting specific fields
- `--fields title` for compact output
- `--hints` drill-down commands
- Snapshot index (85x speedup)
- Bulk mutations (remove from 8,642 files in 2.5s)
- `read --lines` for navigating into specific content
- `--glob` + property filter combos

## Feature ideas

- `hyalo find --count-by <property>`: group-by counts for a property value (currently needs grep | sort | uniq -c)
- `--no-orphans` or `--max-orphans N` for summary on linkless repos
- `--dry-run` for all mutation commands (properties rename, tags rename, bulk set/remove)
- Wikilink sensitivity tuning: option to exclude numeric-only or math-expression bracket patterns from link parsing
