# hyalo

A self-contained command line tool for exploring and managing Markdown knowledge bases. Compatible with [Obsidian](https://obsidian.md/) vaults — no running Obsidian instance required.

## Installation

### Homebrew (macOS & Linux)

```sh
brew tap ractive/tap
brew install ractive/tap/hyalo
```

### Cargo (Intel Mac & other platforms)

```sh
cargo install --git https://github.com/ractive/hyalo.git --locked
```

> **Intel Mac users:** Homebrew bottles and pre-built binaries are only provided for Apple Silicon. If you're on an Intel Mac, use `cargo install` above.

### Manual download

Download pre-built binaries from the [GitHub Releases](https://github.com/ractive/hyalo/releases) page.

## Build

```sh
cargo build --release
```

## Usage

All commands accept `--dir <path>` (default: `.`), `--format json|text` (default: `json`), `--jq <FILTER>` (apply a jq expression to the JSON output), and `--hints` (append executable drill-down command suggestions).

Glob patterns use standard shell semantics: `*` matches within a single directory, `**` matches across directory boundaries. For example, `*.md` matches top-level files only, while `**/*.md` matches all `.md` files recursively.

### Configuration

Place a `.hyalo.toml` file in your working directory to set defaults for global flags:

```toml
# .hyalo.toml
dir = "./my-vault"   # default: "."
format = "text"      # default: "json"
hints = true         # default: false
```

All fields are optional. CLI flags always take precedence over config values. If `.hyalo.toml` is missing, hyalo silently uses built-in defaults; if the file is present but cannot be read or is malformed/invalid, hyalo warns on stderr and falls back to the built-in defaults.

Use `--no-hints` to explicitly disable hints when the config file enables them.

### init

Initialize hyalo in the current project. Creates a `.hyalo.toml` config file with a `dir` setting pointing to your markdown directory.

```sh
# Basic init — creates .hyalo.toml
hyalo init

# Specify the markdown directory explicitly
hyalo init --dir my-vault

# Also set up Claude Code integration (skill + CLAUDE.md hint)
hyalo init --claude
```

Without `--dir`, hyalo auto-detects common documentation directories (`docs/`, `knowledgebase/`, `wiki/`, `notes/`, `content/`, `pages/`) by looking for a subdirectory that contains `.md` files. Falls back to `.` if none is found.

With `--claude`, hyalo additionally:
- Creates `.claude/skills/hyalo/SKILL.md` so Claude Code automatically uses hyalo for markdown operations
- Appends a hyalo usage hint to `.claude/CLAUDE.md`

All steps are idempotent — existing files are skipped, and duplicate hints are not added.

### find

Search and filter files. Returns an array of file objects, each containing frontmatter properties, tags, sections, tasks, and links.

```sh
# All files
hyalo find

# Content search (case-insensitive substring)
hyalo find "retry backoff"
hyalo find "retry" --tag research

# Regex content search (case-insensitive by default)
hyalo find --regexp "retry.*backoff"
hyalo find -e "TODO|FIXME|HACK"
hyalo find -e "fn\s+\w+_test" --tag rust

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

# Filter by section heading (case-insensitive whole-string match)
hyalo find --section "Tasks" --task todo          # open tasks in ## Tasks sections
hyalo find --section "## Design" "TODO"           # content search scoped to level-2 Design sections
hyalo find --section "# Introduction" --fields sections  # level-pinned: only # Introduction, not ## Introduction
hyalo find --section "Tasks" --section "Notes"    # OR: match either section

# Scope to file(s)
hyalo find --file path/to/note.md
hyalo find --glob "notes/*.md"

# Control returned fields (default: all)
hyalo find --fields properties,tags
hyalo find --fields sections,tasks,links

# Sort and limit
hyalo find --sort modified --limit 10
```

### read

Read the body content of a markdown file, optionally filtered by section or line range. Defaults to plain text output.

```sh
# Read full body (text output)
hyalo read --file path/to/note.md

# Read a specific section
hyalo read --file path/to/note.md --section "Proposal"
hyalo read --file path/to/note.md --section "## Proposal"

# Read a line range (1-based, inclusive)
hyalo read --file path/to/note.md --lines 5:10
hyalo read --file path/to/note.md --lines 5:
hyalo read --file path/to/note.md --lines :10

# Show frontmatter only
hyalo read --file path/to/note.md --frontmatter

# JSON output
hyalo read --file path/to/note.md --format json
hyalo read --file path/to/note.md --format json --jq '.content'
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

# Bulk-update: set status on files matching a filter
hyalo set --property status=completed --where-property status=done --glob '**/*.md'

# Add tag to files matching a tag filter
hyalo set --tag reviewed --where-tag research --glob '**/*.md'
```

### remove

Remove frontmatter properties and/or tags from file(s).

```sh
hyalo remove --property status --file path/to/note.md          # remove property
hyalo remove --property tags=serde --file path/to/note.md      # remove value from list
hyalo remove --tag cli --file path/to/note.md
hyalo remove --property status --glob "draft/*.md"

# Remove tag from files matching a property filter
hyalo remove --tag deprecated --where-property status=completed --glob '**/*.md'
```

`remove --property K` (no value) removes the property entirely. `remove --property K=V` removes V from a list property, or removes the property if it is a scalar matching V.

### append

Append values to list properties, promoting scalars to lists if needed.

```sh
hyalo append --property tags=serde --file path/to/note.md
hyalo append --property tags=serde --glob "crates/*.md"

# Append to list property on files matching a tag
hyalo append --property aliases=old-name --where-tag renamed --glob '**/*.md'
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
Files: 32 total
  ".": 5
  "backlog": 7
  "iterations": 12
  "research": 8
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

## Benchmarking

On-demand performance benchmarks using [Criterion](https://github.com/bheisler/criterion.rs) and [Hyperfine](https://github.com/sharkdp/hyperfine):

```sh
cargo bench --bench micro                # pure-function micro-benchmarks
cargo bench --bench vault                # vault-scale benchmarks (needs obsidian-hub)
./bench-e2e.sh                           # end-to-end CLI benchmarks
./bench-e2e.sh target/release/hyalo /tmp/hyalo-baseline   # A/B comparison
```

See `benches/README.md` for full setup, A/B comparison workflows, profiling with [samply](https://github.com/mstange/samply), and memory measurement.

## Releasing

1. Bump the version in `Cargo.toml`
2. Commit: `git commit -am "Bump version to X.Y.Z"`
3. Create a GitHub release with tag `vX.Y.Z` (must match `Cargo.toml`)

The [release workflow](.github/workflows/release.yml) automatically builds binaries for all platforms, uploads them to the release, and updates the Homebrew formula.

## License

MIT
