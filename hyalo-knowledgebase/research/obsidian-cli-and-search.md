---
title: "Obsidian CLI & Search Reference"
type: research
date: 2026-03-20
tags:
  - research
  - cli
  - search
  - obsidian
---

# Obsidian CLI & Search Reference

## CLI Overview

The Obsidian CLI requires the app to be running. Hyalo replaces this with a self-contained tool.

### Parameter Syntax

- Parameters: `parameter=value` (quotes for spaces: `content="Hello world"`)
- Flags: boolean switches (e.g., `open`, `overwrite`)
- File targeting: `file=<name>` (wikilink resolution) or `path=<path>` (exact path)
- Output: `format=json|text|yaml|tsv|csv` depending on command

## Commands Relevant to Hyalo

### Search

```
search query="..." [path=...] [limit=N] [format=text|json] [case]
```

The most powerful command. Supports the full query syntax (see below).

### Properties

```
properties [file=...] [path=...] [format=yaml|json|tsv]
property:read name=<name> [file=...] [path=...]
property:set name=<name> value=<value> [type=text|list|number|checkbox|date|datetime] [file=...] [path=...]
property:remove name=<name> [file=...] [path=...]
```

### Tags

```
tags [file=...] [path=...] [sort=count] [counts] [format=json|tsv|csv]
tag name=<name> [verbose]
```

### Tasks

```
tasks [file=...] [path=...] [status="<char>"] [done] [todo] [format=json|tsv|csv]
task ref=<path:line> [toggle] [done] [todo] [status="<char>"]
task file=<name> line=<n> [toggle] [done] [todo] [status="<char>"]
```

### Links

```
links [file=...] [path=...]                    # outgoing links
backlinks [file=...] [path=...] [format=json]  # incoming links
unresolved [format=json]                        # broken links
orphans                                         # no incoming links
deadends                                        # no outgoing links
```

### Outline

```
outline [file=...] [path=...] [format=tree|md|json]
```

### Move / Rename

```
move file=<name> to=<destination>    # moves file and updates all links
rename file=<name> name=<new-name>   # renames file and updates all links
```

## Search Query Syntax

### Basic Matching

- Words match independently: `meeting work` — files containing both
- Exact phrase: `"star wars"`

### Boolean Logic

- **AND** (default): `meeting work`
- **OR**: `meeting OR work`
- **Negation**: `-work`, `-(work meetup)`
- **Grouping**: `(meeting OR standup) work`

### Operators

| Operator | Purpose | Example |
|----------|---------|---------|
| `file:` | Match filename | `file:.jpg`, `file:202209` |
| `path:` | Match file path | `path:"Daily notes/2022-07"` |
| `content:` | Match file content | `content:"happy cat"` |
| `match-case:` | Case-sensitive | `match-case:HappyCat` |
| `ignore-case:` | Case-insensitive | `ignore-case:ikea` |
| `tag:` | Find by tag | `tag:#work` |
| `line:` | Terms on same line | `line:(mix flour)` |
| `block:` | Terms in same block | `block:(dog cat)` |
| `section:` | Terms in same section | `section:(dog cat)` |
| `task:` | Match in tasks | `task:call` |
| `task-todo:` | Uncompleted tasks | `task-todo:call` |
| `task-done:` | Completed tasks | `task-done:call` |

### Property Queries

```
[property]              # has property
[property:value]        # property equals value
[property:null]         # property is empty
[status:Draft OR Published]
[duration:<5]           # less than
[duration:>5]           # greater than
```

### Regex

JavaScript-flavored regex in forward slashes: `/\d{4}-\d{2}-\d{2}/`

## Hyalo Prioritization

### Must Have (Core Value)

1. **Property queries** — `[type:story] [status:ready]` — this is what grep can't do well
2. **Link graph** — backlinks, outgoing links, unresolved, orphans, deadends
3. **Tag listing/filtering** — aggregate and query tags
4. **Task management** — list, filter, toggle
5. **Move/rename with link updates** — the killer feature over plain file operations

### Should Have

6. **Content search** with boolean logic and operators
7. **Outline extraction**
8. **Output formats** — JSON primarily (for AI agents), also text/yaml

### Nice to Have

9. **Regex search**
10. **Block/section/line scoping** (`block:`, `section:`, `line:`)
11. **Embedded search** (query code blocks)
