# hyalo

A self-contained command line tool for exploring and managing Markdown knowledge bases. Compatible with the [Obsidian CLI](https://obsidian.md/help/cli) — no running Obsidian instance required.

## Build

```sh
cargo build --release
```

## Usage

All commands accept `--dir <path>` (default: `.`) and `--format json|text` (default: `json`).

### Properties

```sh
# List all properties across files
hyalo properties [--glob PATTERN]

# Read a single property
hyalo property read --name NAME --file FILE

# Set a property value
hyalo property set --name NAME --value VALUE [--type TYPE] --file FILE

# Remove a property
hyalo property remove --name NAME --file FILE
```

### Links

```sh
# List outgoing links from a file
hyalo links --file FILE [--resolved | --unresolved]
```

### Tags

Tags are read from and written to the YAML frontmatter `tags` property. Inline `#tags` in body text are not supported.

```sh
# List all unique tags with occurrence counts
hyalo tags [--file FILE | --glob PATTERN]

# Find files containing a specific tag (supports nested matching)
hyalo tag find --name TAG [--file FILE | --glob PATTERN]

# Add a tag to file(s) frontmatter
hyalo tag add --name TAG <--file FILE | --glob PATTERN>

# Remove a tag from file(s) frontmatter
hyalo tag remove --name TAG <--file FILE | --glob PATTERN>
```

**Tag format (Obsidian-compatible):** letters, digits, `_`, `-`, `/`. Must contain at least one non-numeric character. Forward slashes create hierarchy — `tag find --name inbox` matches `inbox`, `inbox/processing`, etc.

## License

MIT
