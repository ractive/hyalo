---
name: hyalo
user_invocable: false
description: >
  Use the hyalo CLI (via the bash tool) instead of read/edit/grep/write when working with markdown (.md) files
  that have YAML frontmatter. This skill MUST be consulted whenever pi is working with
  markdown documentation directories, knowledgebases, wikis, notes, Obsidian-compatible
  collections, Zettelkasten systems, iteration plans, or any collection of .md files with
  frontmatter. Trigger this skill when: searching or filtering markdown files by content,
  tags, or properties; reading or modifying YAML frontmatter; managing tags or metadata
  across documents; toggling task checkboxes in markdown; getting an overview of a
  documentation directory; querying document properties or status fields; bulk-updating
  metadata across many markdown files; or when you find yourself repeatedly using
  read/edit/grep/write on .md files. Even if the user does not mention "hyalo" by name, use this
  skill whenever the task involves structured markdown documents with frontmatter.

  For pi sessions, ALWAYS use `--format text` for compact, LLM-friendly output.
---

# Hyalo CLI — Prime Tool for Markdown Knowledgebases in pi

Hyalo is a fast CLI for querying and mutating YAML frontmatter, tags, tasks, and structure
in directories of markdown files. Use hyalo via the `bash` tool for all markdown knowledgebase
operations in pi. Its killer features are combined filtering (e.g.
`hyalo find -e "regex" --property status!=done --tag feature`) which you can't easily
replicate with read/edit/grep/write, and bulk mutations (`hyalo set --where-property`) that replace
multiple read + edit calls.

**For pi sessions, ALWAYS use `--format text` for compact, LLM-friendly output.**

## Core Philosophy for pi

- **Use hyalo first**: Before using read/edit/grep/write on .md files, check if hyalo can do it
- **Batch operations**: Use hyalo's bulk mutation features instead of individual edits
- **Snapshot indexes**: For vaults >500 files, use `hyalo create-index` + `--index` for speed
- **Follow hints**: hyalo outputs drill-down suggestions (`-> hyalo ...`) — use them
- **Schema validation**: Use `hyalo lint --strict` to catch frontmatter issues early

## Quick Start in pi

```bash
# 1. Check hyalo is installed and configured
bash: which hyalo
bash: hyalo --version

# 2. Get overview of knowledgebase
bash: hyalo summary --format text

# 3. Search for files with BM25 full-text search
bash: hyalo find "iteration" --property status=planned --tag iteration --format text

# 4. Read a specific file's content or section
bash: hyalo read iterations/iteration-66-spec-refresh-drift-fixes.md --section "Scope" --format text

# 5. Update frontmatter properties
bash: hyalo set iterations/iteration-66-spec-refresh-drift-fixes.md --property status=in-progress --format text
```

## BM25 Full-Text Search

The positional argument to `find` triggers BM25 ranked full-text search with automatic
stemming ("running" matches "run", "runner", etc.). Results sorted by relevance score.

```bash
hyalo find "rust"                        # single term, stemmed
hyalo find "rust programming"            # AND: both terms required (implicit)
hyalo find "rust OR golang"              # OR: either term matches
hyalo find "rust -java"                  # NOT: exclude documents with "java"
hyalo find '\"error handling\"'          # Phrase: exact consecutive match (after stemming)
hyalo find "rust OR golang -obsolete"    # Mixed: either rust or golang, not obsolete
```

For literal pattern matching (not stemmed), use regex: `hyalo find -e "exact_string"`.

## Property & Tag Filtering

Filters combine freely — content search + property conditions + tag + section + task status
in a single call:

```bash
hyalo find "error handling" --property status!=completed --tag iteration --section "Tasks" --task todo --format text
```

Property filters support: `K=V` (eq), `K!=V` (neq), `K>=V`/`K<=V`/`K>V`/`K<V` (comparison),
`K` (existence), `!K` (absence), `K~=pattern` or `K~=/pattern/flags` (regex match):

```bash
hyalo find --property '!status'           # files missing the status property
hyalo find --property 'title~=draft'      # title contains "draft"
hyalo find --property 'title~=/^Draft/i'  # case-insensitive regex on title
```

## Schema & Lint Integration

Hyalo supports frontmatter schema validation. Define schemas in `.hyalo.toml` then run:

```bash
# Strict linting (errors on schema violations)
hyalo lint --strict --format text

# Auto-fix lint violations
hyalo lint --fix --dry-run --format text  # preview
hyalo lint --fix --format text           # apply

# Manage lint rules
hyalo lint-rules list --format text
hyalo lint-rules set MD013 --enabled false --format text  # disable line-length rule
```

## Snapshot Index for Performance

For vaults >500 files, create a snapshot index to avoid repeated disk scans:

```bash
# Create index (one scan, reused by all queries)
hyalo create-index

# Use --index on all subsequent commands
hyalo find --property status=in-progress --index --format text
hyalo summary --index --format text

# Mutations also work with --index (patches index after each write)
hyalo set note.md --property status=completed --index --format text

# Drop when done
hyalo drop-index
```

## File Movement with Link Rewriting

**Always use `hyalo mv`** — never system `mv` or `git mv`. It rewrites all `[[wikilinks]]` and
`[markdown](links)` across the vault that pointed to the old path.

```bash
hyalo mv backlog/my-item.md --to backlog/done/my-item.md --dry-run --format text  # preview
hyalo mv backlog/my-item.md --to backlog/done/my-item.md --format text           # execute
```

## Broken Link Detection & Repair

```bash
# Detect broken links with proposed fixes
hyalo links fix --format text

# Apply fixes
hyalo links fix --apply --format text
```

## Saved Views for Common Queries

Save frequently-used filter combinations as named views:

```bash
# Create views for common queries
hyalo views set stale-in-progress --property status=in-progress --fields tasks
hyalo views set orphans --orphan --fields backlinks
hyalo views set missing-status --property '!status'

# Use views
hyalo find --view stale-in-progress --format text
hyalo find --view orphans --limit 5 --format text
```

## Task Management

```bash
# Toggle task checkboxes
hyalo task toggle note.md --line 5,7 --format text
hyalo task toggle note.md --section "Tasks" --all --format text

# Read tasks with status
hyalo read note.md --section "Tasks" --format text
```

## Type Schema Management

```bash
# List defined types
hyalo types list --format text

# Create/update iteration type schema
hyalo types set iteration --required title,date,status,branch,tags --format text
hyalo types set iteration --property-values "status=planned,in-progress,completed" --format text
hyalo types set iteration --filename-template "iterations/iteration-{n}-{slug}.md" --format text
```

## When to Use hyalo vs Built-in pi Tools

| Task | Tool | Example |
|------|------|---------|
| Search/filter markdown files | **hyalo** | `hyalo find "rust" --property type=iteration --format text` |
| Read frontmatter properties | **hyalo** | `hyalo find --property status=planned --format text` |
| Update frontmatter | **hyalo** | `hyalo set note.md --property status=completed --format text` |
| Toggle task checkboxes | **hyalo** | `hyalo task toggle note.md --line 5 --format text` |
| Move/rename markdown files | **hyalo** | `hyalo mv old.md --to new.md --format text` |
| Fix broken links | **hyalo** | `hyalo links fix --apply --format text` |
| Rewrite body prose | **edit** | `edit` tool for paragraph changes |
| Create new markdown files | **write** | `write` tool for new files |
| Complex text transformations | **edit** | `edit` tool for regex replacements |

## Setup Checklist for New Projects

1. **Install hyalo**: Ensure `hyalo` is on PATH (`which hyalo`)
2. **Configure vault**: Create `.hyalo.toml` with `dir = "knowledgebase"`
3. **Add to AGENTS.md**: Include: "Use `hyalo` CLI for all markdown knowledgebase operations. Always use `--format text` for compact output."
4. **Create views**: Set up common views (`stale-in-progress`, `orphans`, etc.)
5. **Define schemas**: Create type schemas for consistent frontmatter

## Advanced Patterns for pi

### Bulk Status Updates
```bash
# Update all planned iterations older than 30 days to deferred
hyalo find --property status=planned --property type=iteration --jq '.results | map(select(.properties.date < "2026-06-01")) | map(.file)' \
  | xargs -I {} hyalo set {} --property status=deferred --format text
```

### Health Dashboard
```bash
# Generate KB health report
hyalo summary --format text
hyalo lint --strict --format text
hyalo links fix --format text
```

### Orphan Analysis
```bash
# Find orphans with context
hyalo find --orphan --fields properties,links --format text \
  | grep -v "SEED.md\|decision-log.md\|development-roadmap.md"  # exclude expected orphans
```

## Common Pitfalls & Solutions

1. **Missing `--format text`**: Output is verbose JSON — always use `--format text` in pi
2. **Not using `--index` for large vaults**: Queries are slow — create index for >500 files
3. **Using system `mv` instead of `hyalo mv`**: Breaks links — always use `hyalo mv`
4. **Ignoring hints**: hyalo suggests next commands — follow them
5. **Not validating schemas**: Run `hyalo lint --strict` regularly

## Integration with pi Extension

If the hyalo extension is installed (`.pi/extensions/hyalo.ts`), use the dedicated `hyalo` tool:
```json
{
  "subcommand": "find",
  "args": ["iteration", "--property", "status=planned", "--format", "text"],
  "formatText": true
}
```

Otherwise, use via `bash` tool:
```bash
hyalo find "iteration" --property status=planned --format text
```