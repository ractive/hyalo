# hyalo

A self-contained command line tool for exploring and managing Markdown knowledge bases. Compatible with [Obsidian](https://obsidian.md/) vaults — no running Obsidian instance required.

## Build

```sh
cargo build --release
```

## Usage

All commands accept `--dir <path>` (default: `.`), `--format json|text` (default: `json`), `--jq <FILTER>` (apply a jq expression to the JSON output), and `--hints` (append executable drill-down command suggestions).

Glob patterns use standard shell semantics: `*` matches within a single directory, `**` matches across directory boundaries. For example, `*.md` matches top-level files only, while `**/*.md` matches all `.md` files recursively.

### find

Search and filter files. Returns an array of file objects, each containing frontmatter properties, tags, sections, tasks, and links.

```sh
# All files
hyalo find

# Content search (case-insensitive substring)
hyalo find "retry backoff"
hyalo find "retry" --tag research

# Filter by property (operator: =, !=, >, >=, <, <=, or existence)
hyalo find --property status=draft
hyalo find --property status!=done
hyalo find --property priority>=3
hyalo find --property status          # existence check (has this property)
hyalo find --property status=draft --property topic=cli   # AND

# Filter by tag (prefix-matches hierarchy: --tag inbox matches inbox/processing)
hyalo find --tag inbox

# Filter by task status
hyalo find --task todo    # open tasks
hyalo find --task done    # completed tasks
hyalo find --task any     # any tasks

# Scope to file(s)
hyalo find --file path/to/note.md
hyalo find --glob "notes/*.md"

# Control returned fields (default: all)
hyalo find --fields properties,tags
hyalo find --fields sections,tasks,links

# Sort and limit
hyalo find --sort modified --limit 10
```

### properties

Aggregate summary of unique property names with inferred types and file counts.

```sh
hyalo properties
hyalo properties --glob "notes/*.md"
```

### tags

Aggregate summary of unique tags with file counts.

```sh
hyalo tags
hyalo tags --glob "notes/*.md"
```

### summary

High-level vault overview: file counts, property and tag aggregates, status groups, tasks, and recently modified files.

```sh
hyalo summary
hyalo summary --glob "notes/*.md"
hyalo summary --recent 5          # control how many recent files to show (default: 10)
hyalo summary --format text
hyalo summary --jq '.tasks.total'
hyalo summary --format text --hints
```

### set

Set (create or overwrite) frontmatter properties and/or add tags across one or more files.

```sh
hyalo set --property status=done --file path/to/note.md
hyalo set --property status=active --glob "notes/*.md"
hyalo set --tag cli --file path/to/note.md
hyalo set --property status=done --tag reviewed --file path/to/note.md
```

### remove

Remove frontmatter properties and/or tags from file(s).

```sh
hyalo remove --property status --file path/to/note.md          # remove property
hyalo remove --property tags=serde --file path/to/note.md      # remove value from list
hyalo remove --tag cli --file path/to/note.md
hyalo remove --property status --glob "draft/*.md"
```

`remove --property K` (no value) removes the property entirely. `remove --property K=V` removes V from a list property, or removes the property if it is a scalar matching V.

### append

Append values to list properties, promoting scalars to lists if needed.

```sh
hyalo append --property tags=serde --file path/to/note.md
hyalo append --property tags=serde --glob "crates/*.md"
```

### task

Read, toggle, or set the status of a single task checkbox by line number.

```sh
hyalo task read --file path/to/note.md --line 42
hyalo task toggle --file path/to/note.md --line 42
hyalo task set-status --file path/to/note.md --line 42 --status /
```

Tasks are markdown checkboxes (`- [ ]`, `- [x]`, `- [/]`, etc.) in the file body. Checkboxes inside fenced code blocks and `%%comment%%` blocks are ignored.

### Hints

Add `--hints` to any read-only command to get suggested drill-down commands:

```
$ hyalo summary --format text --hints
Files: 32 total (.: 5, backlog: 7, iterations: 12, research: 8)
Properties: 8 unique
Tags: 15 unique
Status: completed (10), in-progress (2), planned (2)
Tasks: 89/174

  -> hyalo --dir . properties
  -> hyalo --dir . tags
  -> hyalo --dir . find --task todo
  -> hyalo --dir . find --property status=in-progress
```

In JSON mode, `--hints` wraps the output in `{"data": ..., "hints": [...]}`. Hints are concrete, copy-pasteable commands — no templates or placeholders. Suppressed when combined with `--jq`.

## License

MIT
