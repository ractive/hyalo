# hyalo

A self-contained command line tool for exploring and managing Markdown knowledge bases. Compatible with [Obsidian](https://obsidian.md/) vaults — no running Obsidian instance required.

## Build

```sh
cargo build --release
```

## Usage

All commands accept `--dir <path>` (default: `.`), `--format json|text` (default: `json`), `--jq <FILTER>` (apply a jq expression to the JSON output), and `--hints` (append executable drill-down command suggestions).

Glob patterns use standard shell semantics: `*` matches within a single directory, `**` matches across directory boundaries. For example, `*.md` matches top-level files only, while `**/*.md` matches all `.md` files recursively.

### Properties

```sh
# Aggregate property summary (unique names, types, file counts)
hyalo properties summary [--file FILE | --glob PATTERN]

# Per-file property detail (each file with its key/value pairs)
hyalo properties list [--file FILE | --glob PATTERN]

# Read a single property
hyalo property read --name NAME --file FILE

# Set a property value
hyalo property set --name NAME --value VALUE [--type TYPE] --file FILE

# Remove a property
hyalo property remove --name NAME --file FILE

# Find files by property existence or value
hyalo property find --name NAME [--value VALUE] [--file FILE | --glob PATTERN]

# Add values to a list property (e.g. aliases, authors)
hyalo property add-to-list --name NAME --value VAL... <--file FILE | --glob PATTERN>

# Remove values from a list property
hyalo property remove-from-list --name NAME --value VAL... <--file FILE | --glob PATTERN>
```

`properties summary` is the default when no subcommand is given (`hyalo properties` runs `summary`).

### Links

```sh
# List outgoing links from a file
hyalo links --file FILE [--resolved | --unresolved]
```

### Tags

Tags are read from and written to the YAML frontmatter `tags` property. Inline `#tags` in body text are not supported.

```sh
# Aggregate tag summary (unique tags with file counts)
hyalo tags summary [--file FILE | --glob PATTERN]

# Per-file tag detail (each file with its tags array)
hyalo tags list [--file FILE | --glob PATTERN]

# Find files containing a specific tag (supports nested matching)
hyalo tag find --name TAG [--file FILE | --glob PATTERN]

# Add a tag to file(s) frontmatter
hyalo tag add --name TAG <--file FILE | --glob PATTERN>

# Remove a tag from file(s) frontmatter
hyalo tag remove --name TAG <--file FILE | --glob PATTERN>
```

`tags summary` is the default when no subcommand is given (`hyalo tags` runs `summary`).

**Tag format (Obsidian-compatible):** letters, digits, `_`, `-`, `/`. Must contain at least one non-numeric character. Forward slashes create hierarchy — `tag find --name inbox` matches `inbox`, `inbox/processing`, etc.

### Outline

```sh
# Structural outline of a single file (returns bare object)
hyalo outline --file FILE

# Outline of multiple files (returns array)
hyalo outline --glob PATTERN

# Outline of all .md files under --dir (returns array)
hyalo outline
```

Returns per-file: frontmatter properties (with types and values), tags, and a section tree with heading levels, line numbers, wikilinks, task counts (`total`/`done`), and code block languages. Designed for LLM navigation — understand a document's structure without reading the full content.

### Tasks

```sh
# List tasks (checkboxes) across all files
hyalo tasks

# Tasks in a single file (returns bare object)
hyalo tasks --file FILE

# Tasks matching a glob (returns array)
hyalo tasks --glob PATTERN

# Filter by completion status
hyalo tasks --done           # only completed tasks
hyalo tasks --todo           # only open tasks
hyalo tasks --status x       # tasks with a specific status character

# Single-task operations
hyalo task read --file FILE --line N
hyalo task toggle --file FILE --line N
hyalo task set-status --file FILE --line N --status CHAR
```

Tasks are markdown checkboxes (`- [ ]`, `- [x]`, `- [/]`, etc.) found in the file body. Checkboxes inside fenced code blocks and `%%comment%%` blocks are ignored.

### Summary

```sh
# Vault overview: files, properties, tags, status groups, tasks, recent files
hyalo summary

# Limit to a subset of files
hyalo summary --glob PATTERN

# Control how many recent files to show (default: 10)
hyalo summary --recent 5

# Human-readable text output
hyalo summary --format text

# Extract a single field with --jq
hyalo summary --jq '.tasks.total'

# Show drill-down hints (suggested next commands)
hyalo summary --format text --hints
```

Single-call vault overview designed as the entry point for LLM agents. Returns file counts by directory, property summary, tag counts, status groups, task totals, and recently modified files.

### Hints

Add `--hints` to any read-only command to see executable drill-down commands:

```
$ hyalo summary --format text --hints
Files: 32 total (.: 5, backlog: 7, iterations: 12, research: 8)
Properties: 8 unique
Tags: 15 unique
Status: completed (10), in-progress (2), planned (2)
Tasks: 89/174

  -> hyalo properties summary
  -> hyalo tags summary
  -> hyalo tasks --todo
  -> hyalo property find --name status --value in-progress
```

In JSON mode, `--hints` wraps the output in `{"data": ..., "hints": [...]}`. Hints are concrete, copy-pasteable commands — no templates or placeholders. Suppressed when combined with `--jq`.

## License

MIT
