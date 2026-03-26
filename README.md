# hyalo

Query, filter, and mutate YAML frontmatter across markdown file collections. Compatible with [Obsidian](https://obsidian.md/) vaults, Zettelkasten systems, and any directory of `.md` files with YAML frontmatter — no running Obsidian instance required.

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

All commands accept `-d/--dir <path>` (default: `.`), `--format json|text` (default: `json`), `--jq <FILTER>` (apply a jq expression to the JSON output), `--hints` (append executable drill-down command suggestions), `--site-prefix <PREFIX>` (override the site prefix used for resolving root-absolute links), and `--index <PATH>` (use a pre-built snapshot index instead of scanning files from disk — read-only commands use it; mutation commands ignore it; falls back to disk scan if the index is incompatible).

Most flags have short aliases for quick interactive use:

| Short | Long | Available in |
|-------|------|-------------|
| `-d` | `--dir` | all commands |
| `-e` | `--regexp` | find |
| `-p` | `--property` | find, set, remove, append |
| `-t` | `--tag` | find, set, remove |
| `-s` | `--section` | find, read |
| `-f` | `--file` | find, read, set, remove, append, task, backlinks, mv |
| `-g` | `--glob` | find, set, remove, append, properties summary, properties rename, tags summary, tags rename, summary |
| `-n` | `--limit` | find |
| `-n` | `--recent` | summary |
| `-l` | `--lines` | read |
| `-l` | `--line` | task read, task toggle, task set-status |
| `-s` | `--status` | task set-status |

Glob patterns use standard shell semantics: `*` matches within a single directory, `**` matches across directory boundaries. For example, `*.md` matches top-level files only, while `**/*.md` matches all `.md` files recursively.

### Configuration

Place a `.hyalo.toml` file in your working directory to set defaults for global flags:

```toml
# .hyalo.toml
dir = "./my-vault"   # default: "."
format = "text"      # default: "json"
hints = true         # default: false
site_prefix = "docs" # override auto-derived prefix for absolute link resolution
```

All fields are optional. CLI flags always take precedence over config values. If `.hyalo.toml` is missing, hyalo silently uses built-in defaults; if the file is present but cannot be read or is malformed/invalid, hyalo warns on stderr and falls back to the built-in defaults.

Use `--no-hints` to explicitly disable hints when the config file enables them.

### Absolute link resolution (site prefix)

Documentation sites often use root-absolute links like `/docs/guides/setup.md`. Hyalo resolves these by stripping a site prefix — turning `/docs/guides/setup.md` into the vault-relative path `guides/setup.md`.

By default, hyalo auto-derives the prefix from the last component of `--dir`:

```
--dir ../vscode-docs/docs  →  prefix = "docs"   (/docs/foo.md → foo.md)
--dir /home/me/wiki        →  prefix = "wiki"    (/wiki/foo.md → foo.md)
--dir .                    →  prefix = current directory name (e.g. "wiki")
```

Override when the directory name doesn't match the URL prefix:

```sh
# Directory is "content/" but links use "/docs/..." prefix
hyalo --site-prefix docs --dir ./content find --fields links

# Disable absolute-link resolution entirely
hyalo --site-prefix "" find --fields links
```

Precedence: `--site-prefix` flag > `site_prefix` in `.hyalo.toml` > auto-derived from `--dir`.

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

# Filter by property (operator: =, !=, >, >=, <, <=, existence, absence, or regex)
hyalo find --property status=draft
hyalo find --property status!=done
hyalo find --property priority>=3
hyalo find --property status          # existence check (has this property)
hyalo find --property '!status'       # absence check (missing this property)
hyalo find --property 'title~=draft'  # regex match on property value (unanchored)
hyalo find --property 'title~=/^Draft/i'  # regex with flags (i = case-insensitive)
hyalo find --property status=draft --property topic=cli   # AND

# Filter by tag (prefix-matches hierarchy: --tag inbox matches inbox/processing)
hyalo find --tag inbox

# Filter by task status
hyalo find --task todo    # open tasks
hyalo find --task done    # completed tasks
hyalo find --task any     # any tasks

# Filter by section heading (case-insensitive substring match by default)
hyalo find --section "Tasks" --task todo          # matches "Tasks", "Tasks [4/4]", etc.
hyalo find --section "## Design" "TODO"           # content search scoped to level-2 Design sections
hyalo find --section "# Introduction" --fields sections  # level-pinned: only # Introduction, not ## Introduction
hyalo find --section "Tasks" --section "Notes"    # OR: match either section
hyalo find --section "~=/DEC-03[12]/"             # regex section match

# Scope to file(s) (--file is repeatable)
hyalo find --file path/to/note.md
hyalo find --file a.md --file b.md
hyalo find --glob "notes/*.md"
hyalo find --glob '!**/draft-*'      # exclude files matching a pattern (glob negation)

# Control returned fields (default: all except properties-typed and backlinks)
hyalo find --fields properties,tags
hyalo find --fields sections,tasks,links
hyalo find --fields properties-typed     # [{name, type, value}] array instead of {key: value} map
hyalo find --fields backlinks --file my-note.md    # show who links to this note
hyalo find --fields properties,backlinks           # combine with other fields

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

Subcommand group for property operations.

```sh
# Aggregate summary of unique property names with inferred types and file counts
hyalo properties summary
hyalo properties summary --glob "notes/*.md"

# Bulk rename a property key across all files
hyalo properties rename --from old-key --to new-key
hyalo properties rename --from old-key --to new-key --glob "notes/*.md"
```

### tags

Subcommand group for tag operations.

```sh
# Aggregate summary of unique tags with file counts
hyalo tags summary
hyalo tags summary --glob "notes/*.md"

# Bulk rename a tag across all files
hyalo tags rename --from old-tag --to new-tag
hyalo tags rename --from old-tag --to new-tag --glob "notes/*.md"
```

### summary

High-level vault overview: file counts, property and tag aggregates, status groups, tasks, orphan files (fully isolated — no links in or out), and recently modified files.

```sh
hyalo summary
hyalo summary --glob "notes/*.md"
hyalo summary --recent 5          # control how many recent files to show (default: 10)
hyalo summary --depth 1           # collapse subdirectories beyond depth 1
hyalo summary --format text
hyalo summary --jq '.tasks.total'
hyalo summary --jq '.orphans.files'  # list fully isolated files (no links in or out)
hyalo summary --format text --hints
```

### set

Set (create or overwrite) frontmatter properties and/or add tags across one or more files.

```sh
hyalo set --property status=done --file path/to/note.md
hyalo set --property status=active --glob "notes/*.md"
hyalo set --tag cli --file path/to/note.md
hyalo set --property status=done --tag reviewed --file path/to/note.md

# Set a list-type (YAML sequence) property
hyalo set --property 'authors=[Alice, Bob, Charlie]' --file path/to/note.md

# Multi-file targeting (--file is repeatable)
hyalo set --property status=reviewed --file a.md --file b.md

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

### backlinks

Reverse link lookup — find all files that link to a given file. Scans all `.md` files in the vault and builds an in-memory link graph, then returns every incoming link (both `[[wikilinks]]` and `[markdown](links)`) pointing to the target file.

```sh
hyalo backlinks --file path/to/note.md
```

**JSON output** (default):

```json
{
  "file": "path/to/note.md",
  "backlinks": [
    {
      "source": "index.md",
      "line": 5,
      "target": "note"
    },
    {
      "source": "journal/2026-03-20.md",
      "line": 12,
      "target": "note",
      "label": "project notes"
    }
  ],
  "total": 2
}
```

**Text output** (`--format text`):

```
2 backlinks to path/to/note.md:
  index.md:5
  journal/2026-03-20.md:12 ("project notes")
```

The `label` field (and the parenthesised text in text mode) appears only for aliased wikilinks (`[[target|label]]`) and titled markdown links (`[label](target.md)`).

### mv

Move or rename a markdown file and update all links across the vault. Builds an in-memory link graph, moves the file on disk, then rewrites all inbound `[[wikilinks]]` and `[markdown](links)` in other files that pointed to the old path. Also rewrites relative markdown links inside the moved file whose targets changed due to the new directory context.

```sh
hyalo mv --file old/path.md --to new/path.md
hyalo mv --file note.md --to archive/note.md --dry-run   # preview without writing
```

**JSON output** (default):

```json
{
  "from": "old/path.md",
  "to": "new/path.md",
  "dry_run": false,
  "updated_files": [
    {
      "file": "index.md",
      "replacements": [
        { "line": 5, "old_text": "[[old/path]]", "new_text": "[[new/path]]" }
      ]
    }
  ],
  "total_files_updated": 1,
  "total_links_updated": 1
}
```

**Text output** (`--format text`):

```
Moved old/path.md → new/path.md
Updated 1 link in 1 file:
  index.md:5  [[old/path]] → [[new/path]]
```

Use `--dry-run` to preview which files and links would change without modifying anything.

Root-absolute links (e.g. `/docs/guides/setup.md`) are also rewritten during a move. Hyalo uses the site prefix to map these to vault-relative paths. If `mv` reports 0 links updated but you expect absolute links to be rewritten, check your `--site-prefix` setting (see [Absolute link resolution](#absolute-link-resolution-site-prefix)).

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
Orphans: 3
  "backlog/old-idea.md"
  "research/scratch.md"
  "research/unused-ref.md"

  -> hyalo --dir . properties summary
  -> hyalo --dir . tags summary
  -> hyalo --dir . find --task todo
  -> hyalo --dir . find --property status=in-progress
```

In JSON mode, `--hints` wraps the output in `{"data": ..., "hints": [...]}`. Hints are concrete, copy-pasteable commands — no templates or placeholders. Suppressed when combined with `--jq`.

## Snapshot Index

The snapshot index is a MessagePack file that captures a point-in-time snapshot of the vault's metadata (frontmatter, tags, sections, tasks, links) for faster repeated queries. It is **short-lived and ephemeral** — it becomes stale as soon as any file in the vault is modified.

**Usage:**

```sh
# Create the index (one disk scan)
hyalo create-index

# Run read-only queries against the index (no disk scan)
hyalo find --property status=in-progress --index .hyalo-index
hyalo summary --index .hyalo-index
hyalo tags summary --index .hyalo-index

# Drop the index when done
hyalo drop-index
```

**When to use:** workflows that run many read-only queries in a short window — CI pipelines, automation scripts, LLM tool loops. Create the index at the start, query against it, then drop it.

**Mutations with `--index`:** mutation commands (`set`, `remove`, `append`, `task`, `mv`, `tags rename`, `properties rename`) now support `--index`. They still read and write individual files on disk, but after each mutation they patch the index entry in-place and save it back — keeping the index current for subsequent queries. This is safe as long as no external tool modifies files in the vault while the index is active. If your workflow only uses hyalo for mutations, the index stays consistent across interleaved reads and writes.

Never commit `.hyalo-index` files to version control — they are throwaway artifacts.

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
