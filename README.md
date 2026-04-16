# hyalo

**Your markdown collection deserves a powerful tool to manage it.**

If you maintain an [Obsidian](https://obsidian.md/) vault, a Zettelkasten, documentation site, or any folder of `.md` files with YAML frontmatter, you've probably hit the limits of `grep` and manual editing. Hyalo gives you a fast, structured way to search, filter, and bulk-edit your markdown files from the command line.

### What it does

- **Find files** by frontmatter properties, tags, body content (regex), section headings, task status, or title
- **Bulk-update metadata** — set, remove, or append to properties and tags across hundreds of files at once
- **Move files safely** — rename or reorganize files and hyalo rewrites all `[[wikilinks]]` and `[markdown](links)` across the vault
- **Fix broken links** — detect unresolved links and auto-repair them with fuzzy matching
- **Validate & fix** — lint frontmatter against type schemas, auto-fix defaults, typos, and date formats
- **Read content** — extract specific sections or line ranges from files
- **Get an overview** — see property/tag distributions, task counts, orphan files, and link health at a glance

### Why hyalo?

- **Fast.** Parallel scanning, streaming I/O, optional snapshot index. Handles 10,000+ file vaults in under a second.
- **Structured output.** JSON by default with built-in `--jq` support. Easy to pipe into scripts, CI, or AI agents.
- **AI-agent friendly.** Designed as a tool for Claude Code and other LLM coding agents. One command sets up the integration: `hyalo init --claude`.
- **Safe mutations.** Dry-run mode on all write operations. Preview before committing changes.
- **Cross-platform.** Works on macOS, Linux, and Windows. No runtime dependencies.

> "Hyalo" — short for [hyaloclastite](https://en.wikipedia.org/wiki/Hyaloclastite) — is a volcanic glass, just like obsidian. The project started as a high-performance CLI for [Claude Code](https://claude.ai/claude-code) to maintain Obsidian-compatible knowledgebases.

## Installation

### Homebrew (macOS & Linux)

```sh
brew tap ractive/tap
brew install ractive/tap/hyalo
```

### Scoop (Windows)

```powershell
scoop bucket add hyalo https://github.com/ractive/scoop-hyalo
scoop install hyalo
```

### winget (Windows)

```powershell
winget install ractive.hyalo
```

### Cargo (from crates.io)

```sh
cargo install hyalo-cli
```

> **Intel Mac users:** Homebrew bottles and pre-built binaries are only provided for Apple Silicon. If you're on an Intel Mac, use `cargo install` above.

### Manual download

Download pre-built binaries from the [GitHub Releases](https://github.com/ractive/hyalo/releases) page. Binaries are available for Linux (x86_64, ARM64, glibc and musl), macOS (Apple Silicon), and Windows (x86_64, ARM64).

## Build

```sh
cargo build --release
```

## Usage

All commands accept these global flags:

| Flag | Description |
|------|-------------|
| `-d/--dir <PATH>` | Root directory (default: `.`, override via `.hyalo.toml`) |
| `--format json\|text` | Output format (default: `json`, override via `.hyalo.toml`) |
| `--jq <FILTER>` | Apply a jq expression to the JSON output (incompatible with `--format text`) |
| `--count` | Print total as bare integer — shortcut for `--jq '.total'` (list commands only) |
| `--hints` / `--no-hints` | Enable/disable drill-down command hints (default: on) |
| `--site-prefix <PREFIX>` | Override site prefix for resolving root-absolute links |
| `-q/--quiet` | Suppress warnings on stderr |

Some subcommands also accept snapshot-index flags:

| Flag | Description |
|------|-------------|
| `--index` | Use the snapshot index at `.hyalo-index` in the vault dir (see [Snapshot Index](#snapshot-index)) |
| `--index-file <PATH>` | Use the snapshot index at PATH; implies `--index` |

All JSON output uses a consistent envelope: `{"results": <payload>, "total": N, "hints": [...]}`. `total` is present for list commands (find, tags summary, properties summary, backlinks). `hints` is always present (empty `[]` when `--no-hints`). `--jq` operates on the full envelope, e.g. `--jq '.results[].file'` or `--jq '.total'`.

Most flags have short aliases for quick interactive use:

| Short | Long | Available in |
|-------|------|-------------|
| `-d` | `--dir` | all commands |
| `-e` | `--regexp` | find |
| `-p` | `--property` | find, set, remove, append |
| `-t` | `--tag` | find, set, remove |
| `-s` | `--section` | find, read |
| `-f` | `--file` | find, read, set, remove, append, task, backlinks, mv |
| `-g` | `--glob` | find, set, remove, append, properties summary, properties rename, tags summary, tags rename, summary, links fix |
| `-n` | `--limit` | find, lint, tags summary, properties summary, backlinks |
| `-n` | `--recent` | summary |
| `-l` | `--lines` | read |
| `-l` | `--line` | task read, task toggle, task set |
| `-s` | `--status` | task set |
| `-o` | `--output` | create-index |

Glob patterns use standard shell semantics: `*` matches within a single directory, `**` matches across directory boundaries. For example, `*.md` matches top-level files only, while `**/*.md` matches all `.md` files recursively.

### Configuration

Place a `.hyalo.toml` file in your working directory to set defaults for global flags:

```toml
# .hyalo.toml
dir = "./my-vault"     # default: "."
format = "text"        # default: "json"
hints = false          # default: true (set to false to suppress drill-down hints)
site_prefix = "docs"   # override auto-derived prefix for absolute link resolution
default_limit = 100    # default: 50 (max results for list commands; 0 = unlimited)

# Which list-valued frontmatter properties contribute to the link graph.
# Wikilinks found in these properties' values feed backlinks/orphan/dead-end.
[links]
frontmatter_properties = ["related", "depends-on", "supersedes", "superseded-by"]

# Case-insensitive link resolution: "auto" (default), "true", or "false".
# "auto" probes the filesystem at startup — on case-insensitive filesystems (macOS,
# Windows NTFS) it is enabled automatically; on case-sensitive filesystems it is off.
# "true" forces CI resolution regardless of filesystem (useful in Docker / CI).
# "false" disables it, treating wrong-cased links as unresolved (strict mode).
case_insensitive = "auto"

# Schema validation on write: when true, `set`/`append` behave as if `--validate`
# were always passed — writes that would create lint errors are rejected.
[schema]
validate_on_write = false

# Vault-relative paths or glob patterns to skip during `hyalo lint`. Entries
# without glob meta-characters are matched literally; patterns like
# `vendor/**/*.md` use the same glob semantics as `--glob` elsewhere. Only
# affects the `lint` command — read-only commands still emit parse-error
# warnings for these files.
[lint]
ignore = ["legacy/known-bad.md", "vendor/**/*.md"]
```

All fields are optional. CLI flags always take precedence over config values. If `.hyalo.toml` is missing, hyalo silently uses built-in defaults; if the file is present but cannot be read or is malformed/invalid, hyalo warns on stderr and falls back to the built-in defaults.

**Nested config shadowing:** If the root `.hyalo.toml` has `dir` pointing at a subdirectory that itself contains a `.hyalo.toml`, the nested config is silently shadowed — only the root config applies. Hyalo emits a warning on stderr (`ignoring nested config <path>/.hyalo.toml (shadowed by <root>/.hyalo.toml)`). Either merge the settings into the root config or remove the nested file. Suppress the warning with `--quiet`.

Use `--no-hints` to explicitly disable hints when the config file enables them.

**Default output limits:** List commands (`find`, `lint`, `tags summary`, `properties summary`, `backlinks`) return at most 50 results by default. When results are truncated, the output shows "showing N of M matches" and a hint to get all results. Use `--limit 0` for unlimited output, or set `default_limit` in `.hyalo.toml` to change the project-wide default. The default limit is **not applied** when `--jq` or `--count` is used — programmatic pipelines always receive complete results.

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

With `--claude`, hyalo additionally installs two skills, one rule, and a managed hint section for [Claude Code](https://claude.ai/claude-code):

```
.claude/
├── CLAUDE.md                        # managed section appended (hyalo usage hint)
├── skills/
│   ├── hyalo/SKILL.md               # auto-triggered skill for knowledgebase operations
│   └── hyalo-tidy/SKILL.md          # user-invoked skill for knowledgebase consolidation
└── rules/
    └── knowledgebase.md             # path-triggered rule for markdown files
```

**`hyalo` skill** — Automatically triggered whenever Claude Code works with markdown files in your vault. Teaches Claude to use `hyalo find`, `hyalo set`, `hyalo mv`, etc. instead of raw `Read`/`Edit`/`Grep`/`Glob` — giving it structured access to frontmatter, tags, links, and tasks.

**`hyalo-tidy` skill** — Invoked manually with `/hyalo-tidy`. Runs a five-phase knowledgebase consolidation: orients with `hyalo summary`, gathers recent signal from git history, detects structural issues (broken links, orphan files, stale statuses, missing metadata), applies conservative fixes, and reports a health summary.

**`knowledgebase` rule** — A [path-triggered rule](https://docs.anthropic.com/en/docs/claude-code/settings#rules) scoped to `<your-vault>/**`. Whenever Claude Code touches files in the vault directory, this rule reminds it to prefer `hyalo` CLI commands over built-in file tools.

**`.claude/CLAUDE.md` hint** — A managed section (between `<!-- hyalo:start -->` and `<!-- hyalo:end -->` markers) with a short reminder to use `hyalo` for knowledgebase operations.

All steps are idempotent — re-running `hyalo init --claude` overwrites skills and the rule with the latest versions, and the managed section in `CLAUDE.md` is replaced in-place without duplicating.

### deinit

Remove all artifacts created by `init`. The inverse of `hyalo init --claude`.

```sh
hyalo deinit
```

Removes:
- `.hyalo.toml`
- `.claude/rules/knowledgebase.md`
- `.claude/skills/hyalo/SKILL.md`
- `.claude/skills/hyalo-tidy/SKILL.md`
- The managed section (between `<!-- hyalo:start -->` and `<!-- hyalo:end -->` markers) from `.claude/CLAUDE.md`

Empty parent directories left behind by the removal are cleaned up automatically. The command is idempotent — safe to run when some or all artifacts are already absent.

### find

Search and filter files. Returns a JSON envelope `{"results": [...], "total": N, "hints": [...]}` where each item in `results` contains frontmatter properties, tags, sections, tasks, and links.

```sh
# All files
hyalo find

# Files with broken links (unresolved wikilinks or markdown links)
hyalo find --broken-links

# Orphan files (no inbound or outbound links)
hyalo find --orphan

# Dead-end files (have inbound links but no outbound links)
hyalo find --dead-end

# BM25 ranked full-text search (stemmed, relevance-ranked)
hyalo find "retry backoff"               # AND: both words required
hyalo find "retry OR backoff"            # OR: either word matches
hyalo find "retry -deprecated"           # NOT: exclude "deprecated"
hyalo find '"retry backoff"'             # Phrase: exact consecutive match
hyalo find "retry" --tag research        # combine with filters

# Regex content search (case-insensitive by default, unranked)
hyalo find -e "retry.*backoff"
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
hyalo find --section "/DEC-03[12]/"               # regex section match

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
hyalo find --sort modified --reverse --limit 5      # newest first
hyalo find --sort title                             # sort by title (frontmatter or first H1)
hyalo find --sort date                              # sort by frontmatter date
hyalo find --sort property:priority --reverse       # highest priority first
```

### views

Manage saved views — named filter sets stored in `.hyalo.toml` and recalled with `hyalo find --view <name>`.

```sh
# List all saved views (bare `hyalo views` also works)
hyalo views list

# Save a view (create or overwrite)
hyalo views set drafts --property status=draft
hyalo views set recent-todos --tag project --task todo --sort modified --reverse --limit 20

# Delete a view
hyalo views remove drafts

# Use a saved view
hyalo find --view drafts
hyalo find --view recent-todos --tag rust   # extend the view with additional filters
```

**Merge behavior when combining `--view` with extra flags:**

| Field type | Behavior |
|------------|----------|
| Vec fields (`--property`, `--tag`, `--section`, `--file`, `--glob`) | Extended — extra filters are ANDed on top of the view |
| Option fields (`--sort`, `--limit`, `--regexp`, `--title`, `--task`, `--fields`) | Override — CLI value takes precedence over the view |
| Bool fields (`--broken-links`, `--orphan`, `--dead-end`, `--reverse`) | OR'd — enabled if either the view or the CLI sets them |

**Storage:** Views are persisted as TOML tables in `.hyalo.toml` under `[views.<name>]`:

```toml
[views.drafts]
properties = ["status=draft"]

[views.recent-todos]
tags = ["project"]
task = "todo"
sort = "modified"
reverse = true
limit = 20

[views.orphans]
orphan = true

[views.dead-ends]
dead_end = true
```

`hyalo lint` flags views whose only narrowing key is `fields` (which controls display, not filtering) — add an explicit filter like `orphan = true` or `dead_end = true` to silence the warning.

### read

Read the body content of a markdown file, optionally filtered by section or line range. Defaults to plain text output. `hyalo show` is an alias for `hyalo read`.

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
hyalo read --file path/to/note.md --format json --jq '.results.content'
```

### properties

Subcommand group for property operations.

```sh
# Aggregate summary of unique property names with inferred types and file counts
hyalo properties summary
hyalo properties summary --glob "notes/*.md"
hyalo properties summary --limit 0              # show all (default: 50)

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
hyalo tags summary --limit 0                    # show all (default: 50)

# Bulk rename a tag across all files
hyalo tags rename --from old-tag --to new-tag
hyalo tags rename --from old-tag --to new-tag --glob "notes/*.md"
```

### summary

Compact, fixed-size vault overview: file counts, property and tag aggregates, status counts, tasks, link health, orphan/dead-end counts, and recently modified files. Drill down with `find --orphan`, `find --dead-end`, `find --broken-links`.

```sh
hyalo summary
hyalo summary --glob "notes/*.md"
hyalo summary --recent 5          # control how many recent files to show (default: 10)
hyalo summary --depth 2           # override default depth-1 directory listing
hyalo summary --format text
hyalo summary --jq '.results.tasks.total'
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

# --where-property / --where-tag without --file/--glob defaults to all **/*.md
hyalo set --property status=completed --where-property status=done

# Validate the new value against the schema (rejects enum/pattern violations).
# Also enable globally via [schema] validate_on_write = true in .hyalo.toml.
hyalo set --property status=published --validate --file note.md

# Store a wikilink as a literal string (not parsed as a nested YAML list)
hyalo set --property 'related=[[foo/bar]]' --file note.md
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

# --validate runs schema rules against the appended value before writing
hyalo append --property related=[[baz/qux]] --validate --file note.md
```

`append` does not support `--tag` — tags aren't appendable via `append`. Use `hyalo set --tag <name>` to add a tag instead; running `hyalo append --tag …` prints a hint pointing to `set --tag`.

### task

Read, toggle, or set the status of task checkboxes. Supports three mutually exclusive selectors: `--line` (single, comma-separated, or repeatable), `--section`, and `--all`.

```sh
# Single task
hyalo task read --file note.md --line 42
hyalo task toggle --file note.md --line 42

# Multiple tasks by line number (comma-separated or repeatable)
hyalo task toggle --file note.md --line 5,7,9
hyalo task toggle --file note.md --line 5 --line 7 --line 9

# All tasks under a heading (case-insensitive substring, ##-pinned, or /regex/)
hyalo task toggle --file note.md --section "Acceptance criteria"

# Every task in the file
hyalo task toggle --file note.md --all

# Preview toggle without writing
hyalo task toggle --file note.md --line 5 --dry-run

# Set custom status on all tasks in a section
hyalo task set --file note.md --section Tasks --status /
```

Tasks are markdown checkboxes (`- [ ]`, `- [x]`, `- [/]`, etc.) in the file body. Checkboxes inside fenced code blocks and `%%comment%%` blocks are ignored. Bulk mutations use a single atomic read-modify-write pass.

### backlinks

Reverse link lookup — find all files that link to a given file. Scans all `.md` files in the vault and builds an in-memory link graph, then returns every incoming link (both `[[wikilinks]]` and `[markdown](links)`) pointing to the target file.

```sh
hyalo backlinks --file path/to/note.md
hyalo backlinks --file path/to/note.md --limit 0   # show all (default: 50)
```

**JSON output** (default):

```json
{
  "results": {
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
    ]
  },
  "total": 2,
  "hints": []
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
  "results": {
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
  },
  "hints": []
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

**Targets that are not rewritten** when `mv` updates outbound links inside the moved file:
- Site-absolute links starting with `/` (resolved via site prefix instead)
- URL-scheme links (`http://`, `https://`, `mailto:`, `tel:`, …)
- Fragment-only links (`#heading`)
- Bare non-md tokens with no path separator (e.g. Obsidian-style `[[Topic]]`)

File permissions are preserved across all atomic rewrites (`set`, `task toggle`, `mv`, `lint --fix`) — files keep their existing mode (e.g. `0644`) rather than dropping to `0600`.

### links

Subcommand group for link operations.

```sh
# Preview broken link fixes (dry-run is the default)
hyalo links fix

# Apply fixes to disk
hyalo links fix --apply

# Adjust fuzzy matching threshold (0.0–1.0, default: 0.8)
hyalo links fix --threshold 0.9

# Scope to specific files
hyalo links fix --glob "notes/*.md"

# Skip links that contain Hugo/template syntax
hyalo links fix --ignore-target '{{ ref' --ignore-target '{{ relref'

# Text output
hyalo links fix --format text
```

`links fix` detects broken `[[wikilinks]]` and `[markdown](links)` across the vault and attempts auto-repair using four strategies (in priority order): case-insensitive exact match, extension mismatch (`.md` present/absent), unique stem match anywhere in the vault (shortest-path resolution), and Jaro-Winkler fuzzy match above `--threshold`.

Default is `--dry-run` (preview only). Pass `--apply` to write fixes to disk. Use `--ignore-target` (repeatable) to skip links containing specific substrings — useful for template syntax, external paths, or anchors that aren't real files.

**Case-mismatch detection (`link-case-mismatch`):** When case-insensitive resolution is active (controlled by `[links] case_insensitive` in `.hyalo.toml` — `"auto"`, `"true"`, or `"false"`), `links fix` also reports links whose casing differs from the on-disk filename. These appear as `case_mismatches` in the JSON output and are rewritten to the canonical casing when `--apply` is used. On macOS and Windows (case-insensitive filesystems), `"auto"` enables this automatically.

### lint

Validate frontmatter properties against the schema defined in `.hyalo.toml` (read-only).

```sh
# Lint the whole vault
hyalo lint

# Lint a single file
hyalo lint iterations/iteration-101-bm25.md

# Lint with a glob
hyalo lint --glob "iterations/*.md"

# JSON output
hyalo lint --format json

# Limit output to first 10 files with violations
hyalo lint --limit 10

# Lint only files matching a named type's filename template
hyalo lint --type iteration
```

Lint also warns about comma-joined tags (e.g. `"cli,ux"` instead of separate list items); `--fix` splits them automatically. Lint additionally validates `[views.*]` in `.hyalo.toml` — views whose only narrowing key is `fields` (which picks output columns, not matches) are flagged so you can add an actual filter like `orphan = true` or `tag = [...]` (saved views store tags under the `tag` key, not `tags`).

**Exit codes:** 0 = clean, 1 = errors found, 2 = internal error.

Define a schema in `.hyalo.toml`:

```toml
[schema.default]
required = ["title"]

[schema.types.iteration]
required = ["title", "date", "status", "branch", "tags"]
filename-template = "iterations/iteration-{n}-{slug}.md"

[schema.types.iteration.defaults]
status = "planned"
date = "$today"
type = "iteration"

[schema.types.iteration.properties.status]
type = "enum"
values = ["planned", "in-progress", "completed", "superseded", "shelved", "deferred"]

[schema.types.iteration.properties.branch]
type = "string"
pattern = "^iter-\\d+/"

[schema.types.iteration.properties.date]
type = "date"

[schema.types.iteration.properties.tags]
type = "list"
```

**Property types:** `string` (with optional `pattern` regex), `date` (YYYY-MM-DD), `number`, `boolean`, `list`, `enum` (with `values`).

**Severity levels:**
- `error` — schema violation (missing required, wrong type, invalid enum value, pattern mismatch)
- `warn` — soft issue (no `type` property, no `tags`, property not in schema)

**Schema merging:** `schema.default` is the baseline. Type-specific `required` extends (not replaces) default required. Type-specific `properties` override defaults for the same property name.

When no `[schema]` block is configured, `hyalo lint` exits 0 with zero violations (backwards compatible).

`hyalo summary` shows a one-line lint count (`schema.errors/warnings`) when a schema is configured.

### Lint with auto-fix

```sh
# Preview what lint --fix would change (no files written)
hyalo lint --fix --dry-run

# Apply auto-fixes: insert missing defaults, correct enum typos, normalize dates
hyalo lint --fix

# Fix a single file
hyalo lint --fix my-doc.md
```

Auto-fix handles four categories:
- **Insert defaults** — adds missing required properties with their schema default values
- **Fix enum typos** — corrects near-matches to valid enum values (Levenshtein distance ≤ 2)
- **Normalize dates** — rewrites dates to ISO 8601 (YYYY-MM-DD) format  
- **Infer type** — sets `type` from filename template matches when absent

### types

Manage document-type schemas in `.hyalo.toml` without hand-editing TOML. `types set` is an upsert — it auto-creates the type if it doesn't exist. All mutations preserve existing comments and formatting.

```sh
# List all defined types and their required fields
hyalo types
hyalo types list

# Show the full merged schema for a type
hyalo types show iteration

# Create a new type (or update an existing one) — upsert
hyalo types set iteration --required title,date,status

# Set a default value (auto-applies to existing vault files of that type)
hyalo types set iteration --default 'status=planned' --default 'date=$today'

# Preview changes without writing (dry-run)
hyalo types set iteration --default "status=planned" --dry-run

# Add a property type constraint
hyalo types set iteration --property-type "status=string"
hyalo types set iteration --property-type "date=date"

# Define an enum constraint
hyalo types set iteration --property-values "status=planned,in-progress,completed"

# Set a filename template
hyalo types set iteration --filename-template "iterations/iteration-{n}-{slug}.md"
```

When `--default` is used, hyalo walks all vault files of that type and writes the default value to any file that does not already have the property. Use `--dry-run` to preview which files would be updated.

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

In JSON mode, hints populate the `"hints"` array in the standard envelope: `{"results": ..., "hints": [{"description": "...", "cmd": "hyalo ..."}]}`. The envelope shape is always the same regardless of `--hints`/`--no-hints` — only the array contents change. Each hint has a short description and a concrete, copy-pasteable command. Suppressed when combined with `--jq`.

## Snapshot Index

The snapshot index is a MessagePack file that captures a point-in-time snapshot of the vault's metadata (frontmatter, tags, sections, tasks, links) for faster repeated queries. It is **short-lived and ephemeral** — it becomes stale as soon as any file in the vault is modified outside of hyalo.

**Usage:**

```sh
# Create the index (one disk scan)
hyalo create-index

# Run read-only queries against the index (no disk scan)
# --index alone defaults to .hyalo-index in the vault directory
hyalo find --property status=in-progress --index
hyalo summary --index
hyalo tags summary --index

# Drop the index when done
hyalo drop-index
```

**When to use:** workflows that run many queries in a short window — CI pipelines, automation scripts, LLM tool loops. Create the index at the start, query and mutate against it, then drop it.

**Read-only commands** (`find`, `summary`, `tags summary`, `properties summary`, `backlinks`) skip disk scans entirely when using `--index`.

**Mutation commands** (`set`, `remove`, `append`, `task`, `mv`, `tags rename`, `properties rename`) still read and write individual files on disk, but when `--index` is provided they also patch the index entry in-place after each mutation — keeping the index current for subsequent queries. This is safe as long as no external tool modifies files in the vault while the index is active.

Never commit `.hyalo-index` files to version control — they are throwaway artifacts.

## Common pitfalls

| Mistake | Correct usage |
|---------|--------------|
| `--property 'title=~/pat/'` (Perl-style `=~`) | `--property 'title~=/pat/'` (hyalo uses `~=`) |
| `--property title~=draft` to search all titles | `--title draft` (searches frontmatter title AND H1 headings) |
| `--tag projects` expecting substring match | `--tag project` (prefix match: matches `project`, `project/backend`, but not `projects`) |
| `--glob '/absolute/path/*.md'` | `--glob 'relative/*.md'` (globs are relative to `--dir`) |
| `--format text --jq '.total'` | Remove `--format text` — `--jq` is incompatible with text format |
| `--count --jq '.results'` | Use one or the other — `--count` is a shortcut for `--jq '.total'` |

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
