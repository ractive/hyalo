# hyalo

A self-contained command line tool for exploring and managing Markdown knowledge bases. Compatible with [Obsidian](https://obsidian.md/) vaults — no running Obsidian instance required.

## Build

```sh
cargo build --release
```

## Usage

All commands accept `--dir <path>` (default: `.`) and `--format json|text` (default: `json`).

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

## License

MIT
