---
title: "Frontmatter reformatting on write (key order, list indentation, quotes)"
type: backlog
date: 2026-03-23
status: planned
priority: low
origin: dogfooding iteration-20
tags:
  - backlog
  - frontmatter
  - ux
---

# Frontmatter reformatting on write

## Problem

Any mutation (`set`, `remove`, `append`) rewrites the YAML frontmatter block, causing three cosmetic changes:

1. **Key reordering** — `BTreeMap` sorts alphabetically, so `title, type, date, status` becomes `date, status, title, type`
2. **List indentation** — `  - item` (indented) becomes `- item` (non-indented)
3. **Quote stripping** — `"value"` becomes `value` when quotes aren't syntactically necessary

An add/remove cycle reformats the file even though the net content is unchanged. All changes are semantically identical — no data loss.

## Root cause

- Key ordering: `BTreeMap<String, Value>` sorts keys. Original insertion order is lost at parse time.
- List/quote formatting: `serde_yaml_ng::to_string()` uses its own serialization rules with no configuration for style.
- Accepted in DEC-006: "serde_yaml_ng cannot preserve formatting. Obsidian itself rewrites frontmatter on save."

## Possible fixes

| Issue | Approach | Effort |
|-------|----------|--------|
| Key ordering | Replace `BTreeMap` with `IndexMap` (preserves insertion order) | Medium (~4-6h) |
| List indentation | Raw YAML preservation or post-processing | High |
| Quote style | Raw YAML preservation or different library | High |

## Notes

- The IndexMap swap fixes the most visible issue with moderate effort
- List indentation and quoting require either raw byte-level YAML manipulation or a different YAML library — both high risk
- `serde_yml` is unsafe (RUSTSEC-2025-0068), `yaml-rust2` doesn't preserve formatting either
- The current behavior is deterministic — running the same mutation twice produces identical output
