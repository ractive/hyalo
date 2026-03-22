---
branch: iter-12/cli-redesign
date: 2026-03-22
status: completed
tags:
- iteration
- cli
- search
- llm
- ux
title: 'Iteration 12: CLI Redesign — find/set/remove'
type: iteration
---

# Iteration 12: CLI Redesign — find/set/remove

## Goal

Replace the current 11 commands (~20 subcommands) with a simplified 7-command CLI. One command for reading (`find`), three for writing (`set`/`remove`/`append`), two for aggregation (`properties`/`tags`), and one vault overview (`summary`). Task mutations (`task toggle`/`task set-status`) stay unchanged.

**Research:** [[research/unified-find-command]] (exploration history and design rationale)

## Design Summary

### The File Object

`find` returns an array of file objects. Every markdown file has this shape:

```json
{
  "file": "path.md",
  "modified": "2026-03-21T14:23:45Z",
  "properties": [{"name": "status", "type": "text", "value": "completed"}],
  "tags": ["iteration", "cli"],
  "sections": [{"level": 1, "heading": "Title", "line": 5,
                 "tasks": {"total": 3, "done": 3}, "code_blocks": ["rust"],
                 "links": ["decision-log"]}],
  "tasks": [{"line": 15, "section": "## Implementation", "status": "x",
             "text": "Implement scanner", "done": true}],
  "links": [{"target": "decision-log", "path": "decision-log.md", "label": null}]
}
```

When content search is active, a `matches` field is added:
```json
"matches": [{"line": 23, "section": "## Design", "text": "...matched line..."}]
```

### The Complete CLI

```
READING (per-file)
  find       [PATTERN] [--property K=V]... [--tag T]... [--task STATUS]
             [--glob G | --file F] [--fields F1,F2,...] [--sort S] [--limit N]
             → always returns an array of file objects

READING (aggregates)
  properties [--glob G]         → unique property names, types, counts
  tags       [--glob G]         → unique tags with counts
  summary    [--glob G]         → combined vault overview

WRITING (frontmatter)
  set        --property K=V | --tag T   --file F | --glob G
  remove     --property K[=V] | --tag T  --file F | --glob G
  append     --property K=V              --file F | --glob G

WRITING (tasks)
  task toggle     --file F --line N
  task set-status --file F --line N --status C

GLOBAL FLAGS
  --dir DIR          Root directory
  --format json|text
  --jq FILTER
  --hints
```

### `find` Filters

All filters are AND'd together. Repeatable `--property` and `--tag` flags.

**Content search** — positional argument, case-insensitive substring by default:
```sh
hyalo find "retry backoff"
hyalo find "retry" --tag research
```

**Property filters** — `=` splits name from value (safe because Obsidian property names never contain `=!><`):
```sh
--property status=planned             # equals
--property status!=superseded         # not equals
--property priority>=3                # numeric/date comparison
--property status                     # existence (has this property)
--property status=planned --property topic=refactoring   # multiple (AND)
```

Parsing: split on first `=`. Scan name portion for trailing operator (`!=`, `>=`, `<=`, `>`, `<`). No `=` means existence check. Value can contain anything including `=`.

**Tag filter** — supports nested matching:
```sh
--tag backlog                         # matches backlog, backlog/cli, etc.
```

**Task filter** — `--task STATUS`:
```sh
--task todo     # incomplete tasks (status = space)
--task done     # completed tasks (status = x/X)
--task any      # any tasks at all
--task /        # literal status character
--task '?'      # literal status character
```

### `--fields` Flag

Controls which fields are returned per file. Comma-separated.

```sh
hyalo find --tag backlog --fields tasks,sections
hyalo find --tag backlog --fields properties,tags
```

**Default (no `--fields`): all fields.** Implementation uses a configurable default set so this can be changed to frontmatter-only later if performance demands it.

Available fields: `properties`, `tags`, `sections`, `tasks`, `links`.

`file` and `modified` are always included.

### `--property K=V` in Write Commands

Same parsing as in `find`. Used by `set`, `remove`, `append`:

```sh
hyalo set --property status=done --file F           # set property value
hyalo remove --property status --file F             # remove property entirely
hyalo remove --property deps=serde --file F         # remove "serde" from list, OR remove property if value equals "serde"
hyalo append --property deps=serde --file F         # add "serde" to list property
```

### `remove` Semantics

- `remove --property K` (no value) → removes the property regardless of content
- `remove --property K=V` (with value) → if list: removes V from list. If scalar and value matches: removes property. If scalar and value doesn't match: leaves the property unchanged (file is skipped; no error).

## Commands Removed

| Removed | Replaced by |
|---|---|
| `property find` | `find --property` |
| `property read` | `find --file F` + `--jq` |
| `property set` | `set --property` |
| `property remove` | `remove --property` |
| `property add-to-list` | `append --property` |
| `property remove-from-list` | `remove --property K=V` |
| `tag find` | `find --tag` |
| `tag add` | `set --tag` |
| `tag remove` | `remove --tag` |
| `outline` | `find` (sections field) |
| `tasks` (listing) | `find --task` |
| `links` (per-file) | `find` (links field) |
| `properties list` | `find` (properties field) |
| `properties summary` | `properties` (no subcommand) |
| `tags list` | `find` (tags field) |
| `tags summary` | `tags` (no subcommand) |

## Tasks

- [x] Design `FileObject` struct in `types.rs` (reuse existing `PropertyInfo`, `SectionInfo`, `TaskInfo`, `LinkInfo`)
- [x] Implement `--property K=V` parser (split on first `=`, detect operator suffix)
- [x] Implement `find` command with metadata filters (`--property`, `--tag`, `--task`)
- [x] Implement `--fields` flag with configurable default
- [x] Implement content search (positional PATTERN, `ContentSearchVisitor`)
- [x] Implement `--sort` and `--limit`
- [x] Implement `set` command (property set + tag add)
- [x] Implement `remove` command (property remove + list item remove + tag remove)
- [x] Implement `append` command (add to list property)
- [x] Collapse `properties` to single command (was `properties summary`)
- [x] Collapse `tags` to single command (was `tags summary`)
- [x] Update `summary` to reuse aggregate models
- [x] Remove old commands: `property`, `tag`, `outline`, `tasks`, `links`
- [x] Update help text and cookbook examples
- [x] Add e2e tests for all new commands
- [x] Pass all quality gates (fmt, clippy, test)
- [x] Dogfood with hyalo CLI against `./hyalo-knowledgebase/`

## Design Decisions

- **D1:** `--property K=V` uses `=` as delimiter — safe because Obsidian property names never contain `=!><` characters
- **D2:** Content search is case-insensitive substring by default, `--regex` opt-in for regex
- **D3:** Content search scopes to body only (not frontmatter)
- **D4:** `find` with zero filters returns all files (consistency)
- **D5:** `find` always returns an array, even with `--file` (consistent shape for consumers)
- **D6:** Default `--fields` is all, but configurable for future performance tuning
- **D7:** `matches` field only present when content search is active
- **D8:** Name (`find` vs `search`) — deferred to implementation

## Future Work (Not This Iteration)

**Section-level read/write operations:** Treat sections (by heading) as addressable units for reading and writing body content. Extends `set`/`remove`/`append` from frontmatter to body:

```sh
hyalo read --file F --section "Design Decisions"                          # extract section content
hyalo set --section "Design Decisions" --content "New text..." --file F   # replace section
hyalo append --section "Design Decisions" --content "More text" --file F  # append to section
hyalo remove --section "Design Decisions" --file F                        # remove section
```

This gives LLM agents structured access to markdown body content without line numbers or full-file reads. The `OutlineVisitor` already knows section boundaries.

**Aggregate commands for tasks and links:** `tasks` (total/done/todo breakdown) and `links` (resolved/unresolved/broken targets) as standalone aggregates, currently available only through `summary`.
