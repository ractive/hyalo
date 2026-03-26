---
name: hyalo
user_invocable: false
description: >
  Use the hyalo CLI instead of Read/Edit/Grep/Glob when working with markdown (.md) files
  that have YAML frontmatter. This skill MUST be consulted whenever Claude is working with
  markdown documentation directories, knowledgebases, wikis, notes, Obsidian-compatible
  collections, Zettelkasten systems, iteration plans, or any collection of .md files with
  frontmatter. Trigger this skill when: searching or filtering markdown files by content,
  tags, or properties; reading or modifying YAML frontmatter; managing tags or metadata
  across documents; toggling task checkboxes in markdown; getting an overview of a
  documentation directory; querying document properties or status fields; bulk-updating
  metadata across many markdown files; or when you find yourself repeatedly using
  Grep/Glob/Read on .md files. Even if the user does not mention "hyalo" by name, use this
  skill whenever the task involves structured markdown documents with frontmatter.
---

# Hyalo CLI — Preferred Tool for Markdown with Frontmatter

Hyalo is a fast CLI for querying and mutating YAML frontmatter, tags, tasks, and structure
in directories of markdown files. Its killer features are combined filtering (e.g.
`hyalo find -e "regex" --property status!=done --tag feature`) which you can't easily
replicate with Grep/Glob, and bulk mutations (`hyalo set --where-property`) that replace
multiple Read + Edit calls.

Filters combine freely — content regex + property conditions + tag + section + task status
in a single call, something impossible with Grep/Glob alone:

```bash
hyalo find -e "pattern" --property status!=completed --tag iteration --section "Tasks" --task todo
```

Property filters support: `K=V` (eq), `K!=V` (neq), `K>=V`/`K<=V`/`K>V`/`K<V` (comparison),
`K` (existence), `!K` (absence — files missing the property), `K~=pattern` or `K~=/pattern/flags`
(regex match on value; for list properties, matches if any element matches):

```bash
hyalo find --property '!status'           # files missing the status property
hyalo find --property 'title~=draft'      # title contains "draft"
hyalo find --property 'title~=/^Draft/i'  # case-insensitive regex on title
```

`--section` uses case-insensitive **substring** matching by default — `"Tasks"` matches
`"Tasks [4/4]"`, `"My Tasks"`, etc. Use `"~=/regex/"` for regex. Prefix `##` to pin heading level.

`--glob` supports negation with `!` prefix to exclude files: `--glob '!**/draft-*'`.

The `--fields` flag controls which data is returned. Available fields: `properties`,
`properties-typed`, `tags`, `sections`, `tasks`, `links`, `backlinks`. All fields except
`properties-typed` and `backlinks` are included by default. Both are opt-in: `properties-typed`
returns a `[{name, type, value}]` array instead of a `{key: value}` map; `backlinks` requires
scanning all files to build the link graph. Each backlink entry contains `source` (file path),
`line` (line number), and an optional `label`.

```bash
hyalo find --fields backlinks --file my-note.md       # see who links to this note
hyalo find --fields backlinks --jq 'map(select(.backlinks | length == 0))' # find orphan notes
hyalo find --fields properties,backlinks              # combine with other fields
```

Pipe through `--jq` to reshape output into anything — dashboards, burndowns, reports
(requires JSON format — do not combine with `--format text`):

```bash
hyalo find --property status=in-progress --fields tasks \
  --jq 'map({file, done: ([.tasks[] | select(.status == "x")] | length), total: (.tasks | length)})'
```

**Run `hyalo --help` and `hyalo <command> --help` to learn the full API.**

## Setup (run once per project)

ALWAYS run `which hyalo` as your very first step. Do not skip this.

- **Not on PATH?** Inform the user: "The `hyalo` CLI is not installed. You can install it
  from https://github.com/ractive/hyalo." Fall back to Read/Edit/Grep/Glob.
- **On PATH?** Check for `.hyalo.toml` in the project root. If it exists, hyalo is
  configured — the `dir` setting means you don't need `--dir` on every command.
- **No `.hyalo.toml` but a directory with many `.md` files?** (e.g. `docs/`, `knowledgebase/`,
  `wiki/`, `notes/`, `content/`, or any folder with 10+ markdown files) Suggest creating one:
  ```toml
  dir = "docs"
  ```

**After confirming hyalo works**, add a line to the project's `CLAUDE.md` so future
conversations use hyalo without needing this skill:

```
Use `hyalo` CLI (not Read/Grep/Glob) for all markdown knowledgebase operations (frontmatter, tags, tasks, search). Run `hyalo --help` for usage. Use `--format text` for compact LLM-friendly output.
```

This one-line instruction saves tokens in every future conversation.

## Moving or renaming files

When moving or renaming any file in the knowledgebase, always use `hyalo mv` — never use
system `mv`, `git mv`, or any other tool. `hyalo mv` automatically rewrites all `[[wikilinks]]`
and `[markdown](links)` across the vault that pointed to the old path. Without it, moves
silently break links throughout the knowledgebase.

```bash
# Move a file to a subfolder (updates all links vault-wide)
hyalo mv --file backlog/my-item.md --to backlog/done/my-item.md

# Preview what would change without writing
hyalo mv --file old-path.md --to new-path.md --dry-run
```

## Absolute link resolution (site prefix)

Documentation sites often use root-absolute links like `/docs/guides/setup.md`. Hyalo resolves
these by stripping a **site prefix** — e.g., with prefix `docs`, the link `/docs/guides/setup.md`
becomes the vault-relative path `guides/setup.md`.

**Auto-derived by default** from the last path component of `--dir`:
- `--dir ../vscode-docs/docs` → prefix = `docs`
- `--dir /home/me/wiki` → prefix = `wiki`
- `--dir .` → prefix = name of the current directory (e.g. `wiki`)

**Override when the directory name doesn't match the URL prefix:**
```bash
# Directory is "content/" but links use "/docs/..." prefix
hyalo --site-prefix docs --dir ./content find --fields links

# Disable absolute-link resolution entirely
hyalo --site-prefix "" find --fields links
```

Also settable in `.hyalo.toml` as `site_prefix = "docs"`.
Precedence: `--site-prefix` flag > `.hyalo.toml` > auto-derived from `--dir`.

## When to use hyalo vs. built-in tools

- **hyalo:** queries, frontmatter reads/mutations, tag management, task toggling, bulk updates, **moving/renaming files**
- **Edit tool:** body prose changes (rewriting paragraphs) that hyalo can't handle
- **Write tool:** creating brand new markdown files

Start with `hyalo summary --format text` to orient yourself in a new directory.

## Available commands

- **find** — search/filter by text, regex, property, tag, task status
- **read** — extract body content, a section, or line range
- **summary** — directory overview: file counts, tags, tasks, recent files (use `--depth N` to limit directory listing)
- **properties summary** — list property names and types
- **properties rename** — bulk rename a property key across files (`--from old --to new`)
- **tags summary** — list tags with counts
- **tags rename** — bulk rename a tag across files (`--from old --to new`)
- **set** — create/overwrite frontmatter properties, add tags (supports `--where-property`/`--where-tag` for conditional bulk updates; `--property 'K=[a,b,c]'` creates YAML sequences; `--file` is repeatable)
- **remove** — delete properties or tags
- **append** — add to list properties
- **task** — read, toggle, or set status on checkboxes
- **mv** — move/rename a file and rewrite all inbound links across the vault (`--dry-run` to preview)
- **backlinks** — reverse link lookup: lists all files that link to a given file
- **create-index** — build a snapshot index for faster repeated read-only queries
- **drop-index** — delete a snapshot index file created with create-index

## The --format text flag

Use `--format text` for compact, low-token output designed for LLM consumption — less noise
than JSON, fewer tokens. Reach for it when orienting yourself or scanning results.

**`--format text` and `--jq` are mutually exclusive.** `--jq` operates on JSON, so it requires
the default JSON format. If you need to filter/reshape output, use `--jq` (without `--format text`).
If you just need a quick readable overview, use `--format text` (without `--jq`).

## The backlinks command

Use `hyalo backlinks --file <path>` to find all files that link to a given file (reverse link
lookup). This builds an in-memory link graph by scanning all `.md` files in the directory,
detecting both `[[wikilinks]]` and `[markdown](links)`.

```bash
# Which files reference iteration-37?
hyalo backlinks --file iterations/iteration-37-bulk-mutations.md

# JSON output for programmatic use
hyalo backlinks --file iterations/iteration-37-bulk-mutations.md --format json
```

Supports `--format text` (default, compact) and `--format json`. Useful for impact analysis
(what depends on this file?), finding orphan pages, and navigating link structure.

## Snapshot index for batch queries

When running many read-only queries (e.g., during a dream/consolidation pass or any multi-step
analysis), create a snapshot index first to avoid repeated disk scans:

```bash
# Create the index (one scan, reused by all subsequent queries)
hyalo create-index

# All read-only queries use the index — no disk scan
hyalo find --property status=in-progress --index .hyalo-index
hyalo summary --index .hyalo-index
hyalo tags summary --index .hyalo-index
hyalo backlinks --file some-note.md --index .hyalo-index

# Drop the index when done
hyalo drop-index
```

The index is **ephemeral** — create it, use it, drop it within the same session. It becomes
stale the moment any file in the vault changes. Never persist it across sessions.

**For mixed read/write workflows:** gather all information with `--index` first, then drop the
index, then execute mutations. Mutation commands (`set`, `remove`, `append`, `task`, `mv`,
`tags rename`, `properties rename`) ignore `--index` — they always scan fresh from disk.
