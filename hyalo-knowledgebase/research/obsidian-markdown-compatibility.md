---
date: 2026-03-20
status: reference
tags:
- research
- markdown
- obsidian
title: Obsidian Markdown Compatibility
type: research
---

# Obsidian Markdown Compatibility

Obsidian uses three layers of Markdown: CommonMark (base), GFM (extensions), and Obsidian-specific syntax.

## Standard CommonMark

Paragraphs, headings (`#`–`######`), bold (`**`), italic (`*`), links, images, blockquotes, lists, horizontal rules, inline code, fenced code blocks, escaping.

## GFM Additions (Widely Supported)

- Tables (pipe syntax with alignment)
- Strikethrough (`~~text~~`)
- Task lists (`- [ ]` / `- [x]`)
- Autolinks

## Obsidian-Specific Syntax

These are the elements hyalo needs to **parse but not render**:

### Internal Links (Wikilinks)

```md
[[Note Name]]
[[Note Name|Display Text]]
[[Note Name#Heading]]
[[Note Name#^block-id]]
```

Markdown-style alternative: `[Display](Note%20Name.md)`

### Embeds

```md
![[Note Name]]
![[Note Name#Heading]]
![[Note Name#^block-id]]
![[image.png]]
```

### Block References

```md
This is a block. ^block-id
```

Referenced via `[[Note#^block-id]]` or `![[Note#^block-id]]`.

### Highlights

```md
==highlighted text==
```

### Comments

```md
%%inline comment%%

%%
Block comment
spanning multiple lines
%%
```

Only visible in editing view. Hyalo should skip these in content search (or make it configurable).

### Callouts

```md
> [!info] Title
> Content with **Markdown** and [[wikilinks]].

> [!warning]- Foldable (collapsed by default)
> Hidden content.

> [!tip]+ Foldable (expanded by default)
> Visible content.
```

Supported types: `note`, `abstract` (summary, tldr), `info`, `todo`, `tip` (hint, important), `success` (check, done), `question` (help, faq), `warning` (caution, attention), `failure` (fail, missing), `danger` (error), `bug`, `example`, `quote` (cite). Case-insensitive. Unknown types default to `note`.

### Tags

Inline: `#tag`, `#nested/tag`

Rules:
- Must contain at least one non-numeric character
- Allowed: letters, numbers, `_`, `-`, `/`
- No spaces
- Case-insensitive (display preserves first-seen casing)
- Nested via `/`: searching `#inbox` also matches `#inbox/to-read`

Also settable in frontmatter (see [[obsidian-properties]]).

### Task Lists (Extended)

GFM only recognizes `[ ]` and `[x]`. Obsidian supports any character:

```md
- [ ] Todo
- [x] Done
- [-] Cancelled
- [?] Question
- [/] In progress
```

### Other

- Image resizing: `![alt|100x145](url)` or `![alt|100](url)`
- Inline footnotes: `^[This is an inline footnote.]`
- Mermaid diagrams in fenced code blocks (with `internal-link` class for linking nodes to notes)
- Math: `$inline$` and `$$block$$` via MathJax
- Markdown inside HTML blocks is **not** rendered

## Compatibility Summary

| Feature | Standard | Hyalo Must Parse? |
|---------|----------|-------------------|
| Wikilinks `[[]]` | Obsidian | Yes (critical for links/backlinks) |
| Embeds `![[]]` | Obsidian | Yes (for link graph) |
| Block refs `^id` | Obsidian | Nice-to-have |
| Highlights `==` | Obsidian | No (display only) |
| Comments `%%` | Obsidian | Yes (skip in search) |
| Callouts `[!type]` | Obsidian | Nice-to-have |
| Tags `#tag` | Obsidian | Yes (critical for tag commands) |
| Extended tasks `[?]` | Obsidian | Yes (for task commands) |
| Frontmatter YAML | CommonMark ext | Yes (critical) |
| Tables | GFM | No (display only) |
| Math `$...$` | Extended | No (display only) |
