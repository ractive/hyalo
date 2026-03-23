---
date: 2026-03-20
status: reference
tags:
- research
- frontmatter
- properties
- obsidian
title: Obsidian Properties (Frontmatter)
type: research
---

# Obsidian Properties (Frontmatter)

Properties are YAML frontmatter at the very beginning of a note, delimited by `---`.

## Format

```yaml
---
title: My Note
tags:
  - journal
  - personal
status: draft
priority: 3
published: true
date: 2026-03-20
updated: 2026-03-20T10:30:00
---
```

JSON is also accepted between `---` but Obsidian converts it to YAML on save. Hyalo should support both for reading.

## Supported Property Types

| Type | YAML Example | Notes |
|------|-------------|-------|
| **Text** | `title: A New Hope` | Single-line string. No markdown rendering. |
| **List** | `tags:\n  - a\n  - b` | Multiple values, each `- item` |
| **Number** | `priority: 3` or `pi: 3.14` | Integers and decimals. No expressions. |
| **Checkbox** | `published: true` | `true`, `false`, or blank (indeterminate) |
| **Date** | `date: 2026-03-20` | `YYYY-MM-DD` format |
| **Date & Time** | `updated: 2026-03-20T10:30:00` | ISO 8601 |
| **Tags** | `tags:\n  - recipe` | Exclusive to the `tags` property name |

## Special/Default Properties

| Property | Type | Purpose |
|----------|------|---------|
| `tags` | Tags (list) | Tag management |
| `aliases` | List | Alternative names for the note (used in link autocomplete) |
| `cssclasses` | List | CSS classes for styling (Obsidian-specific, irrelevant for hyalo) |

## Global Type Assignment

In Obsidian, once a property name gets a type, **all notes in the vault use that type** for that property name. This is an Obsidian UI behavior — in the file system, types are implicit from the YAML values.

**Implication for hyalo:** We infer types from YAML values. No need for a global type registry initially, but an index could track property schemas for validation later.

## Constraints

- No nested YAML objects — only flat key-value pairs
- No markdown rendering in property values
- Internal links in properties must be quoted: `link: "[[Note Name]]"`
- Numbers must be literals (no expressions)
- Hashtags in text properties do **not** create tags
- Property names must be unique per note

## Search Integration

Obsidian search syntax for properties:

```
[property]           # files that have the property
[property:value]     # property equals value
[property:null]      # property exists but is empty/blank
[status:Draft OR Published]  # boolean logic in values
[duration:<5]        # less than (numeric comparison)
[duration:>5]        # greater than (numeric comparison)
```

## Implications for Hyalo

1. **Parser** must handle YAML frontmatter extraction and type inference
2. **Property commands** need: `read`, `set`, `remove`, `list`
3. **Type coercion** on `set`: infer or accept explicit type parameter
4. **Search** must support `[property:value]` syntax with comparisons
5. **Serialization** must preserve existing YAML formatting where possible (don't rewrite the entire frontmatter just to change one value)
