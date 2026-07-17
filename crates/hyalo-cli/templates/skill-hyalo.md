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

Filters combine freely — content search + property conditions + tag + section + task status
in a single call, something impossible with Grep/Glob alone:

```bash
hyalo find "error handling" --property status!=completed --tag iteration --section "Tasks" --task todo
```

## BM25 Full-Text Search

The positional argument to `find` triggers BM25 ranked full-text search with automatic
stemming ("running" matches "run", "runner", etc.). Results are sorted by relevance score
by default (unless `--sort` is specified).

```bash
hyalo find "rust"                        # single term, stemmed
hyalo find "rust programming"            # AND: both terms required (implicit)
hyalo find "rust OR golang"              # OR: either term matches
hyalo find "rust -java"                  # NOT: exclude documents with "java"
hyalo find '"error handling"'            # Phrase: exact consecutive match (after stemming)
hyalo find '"error handling" -panic'     # Phrase + negation combined
hyalo find "rust OR golang -obsolete"    # Mixed: either rust or golang, not obsolete
```

For literal pattern matching (not stemmed), use regex: `hyalo find -e "exact_string"`.

Stemmer language: `--stemmer french` (or the older `--language french`) selects the French
Snowball stemmer for BM25 tokenization. Accepts full names (english, german, french, …) or
ISO 639-1 codes (en, de, fr, …). This is *not* markdown code-block language filtering.
Per-file override via frontmatter `language: french`. Config default via
`[search] language = "french"` in `.hyalo.toml`.

Property filters support: `K=V` (eq), `K!=V` (neq), `K>=V`/`K<=V`/`K>V`/`K<V` (comparison),
`K` (existence), `!K` (absence — files missing the property), `K~=pattern` or `K~=/pattern/flags`
(regex match on value; for list properties, matches if any element matches):

```bash
hyalo find --property '!status'           # files missing the status property
hyalo find --property 'title~=draft'      # title contains "draft"
hyalo find --property 'title~=/^Draft/i'  # case-insensitive regex on title
```

`--title` filters by the displayed title (frontmatter `title` or first H1 heading).
Case-insensitive substring by default; use `"/regex/"` for regex. Note: `--title` checks
the *displayed* title, while `--property title~=` only checks the frontmatter property.

```bash
hyalo find --title "meeting"           # substring match on displayed title
hyalo find --title "/^Design/i"        # regex on displayed title
```

`--section` uses case-insensitive **substring** matching by default — `"Tasks"` matches
`"Tasks [4/4]"`, `"My Tasks"`, etc. Use `"/regex/"` for regex. Prefix `##` to pin heading level.

`--glob` supports negation with `!` prefix to exclude files: `--glob '!**/draft-*'`.

`--sort` controls result ordering. Available: `file` (default), `modified`, `date`, `title`,
`backlinks_count`, `links_count`, or `property:<KEY>` for any frontmatter property. Add
`--reverse` to flip the direction.

```bash
hyalo find --sort modified --reverse --limit 10   # recently modified files
hyalo find --sort property:priority                # sort by custom property
hyalo find --sort backlinks_count --reverse        # most-linked files first
```

The `--fields` flag controls which data is returned. Available fields: `properties`,
`properties-typed`, `tags`, `sections` (alias: `outline`), `tasks`, `links`, `backlinks`, `title`. Default fields are
`properties`, `tags`, `sections`, `links`. Opt-in fields: `tasks`, `properties-typed`,
`backlinks`, `title`. Use `--fields all` or `--fields tasks` to include them. `properties-typed`
returns a `[{name, type, value}]` array instead of a `{key: value}` map; `backlinks` requires
scanning all files to build the link graph. Each backlink entry contains `source` (file path),
`line` (line number), and an optional `label`.

```bash
hyalo find --fields backlinks --file my-note.md       # see who links to this note (--file required: positional is PATTERN)
hyalo find --orphan                                        # find orphan files (no inbound or outbound links)
hyalo find --dead-end                                      # find dead-end files (inbound but no outbound links)
hyalo find --broken-links                                  # find files with at least one unresolved link
hyalo find --fields properties,backlinks              # combine with other fields
```

**All JSON output uses a consistent envelope:** `{"results": <payload>, "total": N, "hints": [...]}`.
`total` is present for list commands (find, tags summary, properties summary, backlinks).
`hints` is always present (empty `[]` when `--no-hints`). `--jq` operates on the full envelope:

```bash
hyalo find --property status=draft --count                 # count matching files (bare integer)
hyalo find --property status=draft --jq '.total'           # same, via jq
hyalo find --property status=draft --jq '.results[].file'  # just file paths
hyalo summary --jq '.results.tasks.total'                  # tasks count from summary
```

**Hints are enabled by default.** Every query appends drill-down suggestions (`-> hyalo ...`
lines in text mode, a `"hints"` array in the JSON envelope). Read and follow these hints — they show
concrete next commands to explore deeper. Use `--no-hints` to suppress them, or `--jq` which
suppresses hints automatically.

Pipe through `--jq` to reshape output into anything — dashboards, burndowns, reports.
`--jq` requires JSON output; piping naturally produces JSON, so `--jq` works without
an explicit `--format json` in most contexts:

```bash
hyalo find --property status=in-progress --fields tasks \
  --jq '.results | map({file, done: ([.tasks[] | select(.status == "x")] | length), total: (.tasks | length)})'
```

**Run `hyalo --help` and `hyalo <command> --help` to learn the full API.**

## Always run hyalo from the project root

Hyalo reads `dir` from `.hyalo.toml` at the project root, so it already knows where the
knowledgebase lives. You never need to `cd` anywhere or use absolute paths — and doing so
is actively wrong.

- **ALWAYS run hyalo from the project root** (the directory that contains `.hyalo.toml`).
  Never `cd` into the configured `dir` first.
- **ALWAYS pass `--file` paths relative to the configured `dir`** (e.g.
  `iterations/iteration-17.md`). Never pass an absolute path.

Worked example:

```bash
# ✅ Right (from project root — hyalo resolves the path against `dir`)
hyalo set iterations/iteration-17.md --property status=in-progress

# ❌ Wrong (cd into the configured dir — hyalo gets confused about the vault root)
cd hyalo-knowledgebase && hyalo set iterations/iteration-17.md --property status=in-progress

# ❌ Wrong (absolute path — bypasses the configured `dir` entirely)
hyalo set --file /Users/me/proj/hyalo-knowledgebase/iterations/iteration-17.md --property status=in-progress
```

Hyalo emits a stderr warning when it detects either anti-pattern (running from inside the
configured `dir`, or being passed an absolute `--file` path). **Treat that warning as a
correction signal**: stop, move back to the project root, and rewrite the path as a
vault-relative one before continuing.

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
Use `hyalo` CLI (not Read/Grep/Glob) for all markdown knowledgebase operations (frontmatter, tags, tasks, search). Run `hyalo --help` for usage. Output format auto-detects (text on terminals, json when piped); pass `--format text`/`--format json` to override.
```

This one-line instruction saves tokens in every future conversation.

## Moving or renaming files

When moving or renaming any file in the knowledgebase, always use `hyalo mv` — never use
system `mv`, `git mv`, or any other tool. `hyalo mv` automatically rewrites all `[[wikilinks]]`
and `[markdown](links)` across the vault that pointed to the old path. Without it, moves
silently break links throughout the knowledgebase.

```bash
# Move a file to a subfolder (updates all links vault-wide)
hyalo mv backlog/my-item.md --to backlog/done/my-item.md

# Preview what would change without writing
hyalo mv old-path.md --to new-path.md --dry-run
```

`hyalo mv` rewrites relative `.md` paths only. It leaves untouched: site-absolute links
(`/docs/...`, handled separately via site prefix), URL-scheme links (`http://`, `mailto:`),
fragment-only links (`#section`), and bare non-`.md` wiki tokens. File permissions (e.g.
`0644`) are preserved through all atomic rewrites.

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

- **hyalo:** queries, frontmatter reads/mutations, tag management, task toggling, bulk updates, **moving/renaming files**, extracting sections
- **Edit tool:** body prose changes (rewriting paragraphs) that hyalo can't handle
- **Write tool:** creating brand new markdown files

Use `hyalo read` to extract file content without opening the full file:

```bash
hyalo read my-note.md                              # full body (no frontmatter)
hyalo read my-note.md --section "Tasks"            # extract one section
hyalo read my-note.md --lines 1:20                 # line range (1-based)
hyalo read my-note.md --frontmatter                # include YAML frontmatter
```

Start with `hyalo summary` to orient yourself in a new directory (text output is the
default in interactive terminals).

## Available commands

- **find** — BM25 ranked full-text search (AND, OR, phrase, negation) or regex; filter by property, tag, task status
- **read** — extract body content, a section, or line range
- **summary** — compact fixed-size orientation view: file counts, tags, tasks, orphans, dead-ends, links, schema lint count (use `--depth N` to override directory depth)
- **lint** — validate frontmatter against the `[schema]` and lint markdown body with mdbook-lint MD001..MD059 + HYALO001/002 native rules; supports `--rule`, `--rule-prefix`, `--detailed`, `--max-per-rule`, `--strict` (promotes missing-type and undeclared-property warnings to errors), `--fix`, `--fix-rule`; exit 1 when errors found
- **lint-rules** — manage which lint rules are enabled and their severity in `.hyalo.toml` (list, show, set, remove)
- **types** — manage `[schema.types.*]` entries in `.hyalo.toml` (list, show, set, remove)
- **properties summary** — list property names and types
- **properties rename** — bulk rename a property key across files (`--from old --to new`)
- **tags summary** — list tags with counts
- **tags rename** — bulk rename a tag across files (`--from old --to new`)
- **set** — create/overwrite frontmatter properties, add tags (supports `--where-property`/`--where-tag` for conditional bulk updates, which default to all `**/*.md` when no `--file`/`--glob` is given; `--property 'K=[a,b,c]'` creates YAML sequences; `--property 'K=[[foo/bar]]'` stores a literal wikilink string; `--validate` rejects values that would fail schema lint; file arg is positional or `--file`, repeatable)
- **remove** — delete properties or tags
- **append** — add to list properties (supports `--validate`; note: tags are not appendable; use `set --tag` instead)
- **task** — read, toggle, or set status on checkboxes (supports `--line 5,7`, `--section "Tasks"`, `--all`; `--dry-run` to preview toggles)
- **mv** — move/rename a file and rewrite all inbound links across the vault (`--dry-run` to preview)
- **links fix** — detect broken wikilinks/markdown links and auto-repair (dry-run by default; `--apply` to write). Also detects case mismatches when `[links] case_insensitive` is enabled (default `"auto"` on macOS/Windows)
- **backlinks** — reverse link lookup: lists all files that link to a given file
- **create-index** — build a snapshot index for faster repeated read-only queries
- **drop-index** — delete a snapshot index file created with create-index
- **views list** — show all saved views and their filters
- **views set** — save a find query as a named view (`hyalo views set todo --task todo`)
- **views remove** — delete a saved view

## Schema & Lint

`hyalo lint` runs two passes in one invocation:

1. **Frontmatter** — validates against the `[schema]` block in `.hyalo.toml`. No-op when no schema is configured.
2. **Markdown body** — stock mdbook-lint rules (MD001..MD059) plus two HYALO native rules:
   - **HYALO001** — bare `[]` should be `- [ ]` (autofixable)
   - **HYALO002** — `status: completed` requires all task checkboxes ticked (fires only when `[schema.types.*].properties.status` is declared as an enum containing `"completed"`)

```bash
hyalo lint                               # whole vault, summary mode
hyalo lint iterations/iter-42.md         # one file
hyalo lint --fix --dry-run               # preview autofixes
hyalo lint --fix                         # apply
```

Use `hyalo lint --help` for narrowing flags (`--rule`, `--rule-prefix`, `--detailed`, `--max-per-rule`, `--fix-rule`, etc.). The snapshot index does **not** accelerate the body pass.

**Strict mode:** `hyalo lint --strict` (or `[lint] strict = true` in `.hyalo.toml`)
promotes the "no `type` property" and "undeclared property in frontmatter" warnings to
errors, so lint exits non-zero on those cases. Useful in CI and `/hyalo-tidy` to fail
fast on schema drift.

**GitHub PR annotations:** `hyalo lint --strict --format github` (lint-only) emits
`::error`/`::warning file=…,line=…,title=<RULE_ID>::<message>` GitHub Actions workflow
commands so violations render as inline PR annotations, plus a one-line summary. Paths are
repo-root-relative, so run it from the repository root. Composes with `--files-from -` for a
diff-aware variant. Other subcommands reject `--format github`.

**Tune which rules run with `hyalo lint-rules`** (list / show / set / remove). Reach for it when a rule is too noisy on your KB style — disable it or change its severity rather than living with the warnings:

```bash
hyalo lint-rules list                          # see what's enabled
hyalo lint-rules set MD013 --enabled false     # turn one off
hyalo lint-rules set HYALO001 --severity error # promote to error
```

Lint also warns about comma-joined tags (e.g. `tags: ["cli,ux"]` instead of two list
items); `--fix` splits them into proper list entries automatically.

Lint additionally validates saved views in `.hyalo.toml`: if a `[views.*]` entry only
sets `fields` (which controls output columns, not which files match), lint flags it so
you can add a real filter like `orphan = true` or `tag = [...]` (saved views
store tags under the `tag` key).

Exit codes: 0 = clean, 1 = errors found, 2 = internal error.

**Schema format:**

```toml
[schema.default]
required = ["title"]

[schema.types.iteration]
required = ["title", "date", "status", "branch", "tags"]

[schema.types.iteration.properties.status]
type = "enum"
values = ["planned", "in-progress", "completed", "superseded"]

[schema.types.iteration.properties.date]
type = "date"

[schema.types.iteration.properties.branch]
type = "string"
pattern = "^iter-\\d+/"
```

Property types: `string` (optional `pattern` regex), `date` (YYYY-MM-DD), `datetime` (naive YYYY-MM-DDThh:mm:ss, no offset), `datetime-tz` (timezone-aware RFC 3339: YYYY-MM-DDThh:mm:ss plus `Z` or `±hh:mm`, e.g. `2026-05-28T22:44:47+00:00`), `number`, `boolean`, `list`, `string-list` (optional `item_pattern` regex), `enum` (with `values`). `datetime` and `datetime-tz` are disjoint — a naive value never satisfies `datetime-tz` and vice-versa.

Reserved-file exemption: `[schema] exempt = ["**/index.md", "**/log.md"]` binds matching files to no schema (they skip missing-`type`, required-property, and undeclared-property checks). Globs are vault-relative and cross-platform.

**`required` empty-value semantics:** a required property whose value is YAML null (`tags: ~`) or an empty array (`tags: []`) is an error (`required property "tags" must not be empty`). Vacuous values convey no information for a required field, so they're treated as semantically equivalent to absent. This fires regardless of declared constraint type. Atomic-typed required properties (`string`, `date`, `number`, ...) only need to be present — an empty string or zero still satisfies them. So `required = ["tags"]` + `type = "list"` is the idiomatic way to enforce non-empty tags; no separate `min_items` knob exists.

When no `[schema]` block exists, lint exits 0 with zero violations (backwards compatible).

`hyalo summary` includes a `schema` field with error/warning counts when a schema is configured.

**Validate on write:** `hyalo set` and `hyalo append` accept `--validate` to reject values
that would fail lint. Enable globally via `[schema] validate_on_write = true` in `.hyalo.toml`.

**Ignore known-bad files:** add `[lint] ignore = ["legacy/known-bad.md", "vendor/**/*.md"]`
to `.hyalo.toml` to skip listed files during `hyalo lint` (plain strings match literally;
glob meta-characters use `--glob` semantics). Read-only commands still warn on parse errors.

`hyalo lint --count` returns just the number of files with violations.

## Types — manage type schemas

`hyalo types` manages `[schema.types.*]` entries in `.hyalo.toml` without hand-editing TOML. All mutations preserve existing comments and formatting.

```bash
hyalo types list                                     # list all defined types
hyalo types show iteration                           # full merged schema for a type
hyalo types remove iteration                         # remove a type entry
hyalo types set iteration --required title,date      # create or update type (upsert)
hyalo types set iteration --default "status=planned" # set default (auto-applies to vault files)
hyalo types set iteration --property-type "date=date"
hyalo types set iteration --property-values "status=planned,in-progress,completed"
hyalo types set iteration --filename-template "iterations/iteration-{n}-{slug}.md"
hyalo types set iteration --required branch --dry-run  # preview without writing
```

`types set` is an upsert — it auto-creates the type if it doesn't exist. When adding `--required` fields, string property constraints are auto-created for fields without an explicit constraint.

When `--default` is used, hyalo applies the default to all vault files of that type missing that property.

## Views — saved find queries

Views save frequently-used filter combinations under a name in `.hyalo.toml`.
They compose: CLI flags passed alongside `--view` extend or override the saved filters.

**Before constructing a complex `hyalo find`, check if a matching view exists:**
```bash
hyalo views list
```

**If you run the same multi-filter find command 3+ times, save it as a view:**
```bash
hyalo views set stale-iterations --property type=iteration --property status=in-progress
hyalo views set perf-research "performance" --tag research   # BM25 pattern + filter
hyalo views set orphans --orphan                             # files with no inbound/outbound links
hyalo views set dead-ends --dead-end                         # files with inbound but no outbound links
hyalo find --view stale-iterations                    # reuse later
hyalo find --view stale-iterations --limit 5          # compose with overrides
```

hyalo suggests saving non-trivial queries as views in its hint output — follow those hints.

**Manage views:**
- `hyalo views list` — show all saved views
- `hyalo views set <name> [filters...]` — create or update a view
- `hyalo views remove <name>` — delete a view
- `hyalo find --view <name> [extra filters...]` — use a view, optionally with overrides

## Output format

Output format is auto-detected — `text` for interactive terminals, `json` when piped.
Pass `--format text` or `--format json` to override, or set a default in `.hyalo.toml`
(`format = "text"` / `format = "json"`). An explicit `--format` flag always wins.

`text` is the compact, low-token format designed for LLM consumption — less noise than
JSON, fewer tokens. Use it when orienting yourself or scanning results.

**`--format text` and `--jq` are mutually exclusive.** `--jq` operates on JSON, so it
requires JSON output. Piping naturally produces JSON, so `--jq` works without an
explicit flag in most contexts. If you need to filter/reshape output, just pipe through
`--jq`. If you want a readable overview, rely on the auto-default (or pass
`--format text` explicitly when piping to a pager).

## The backlinks command

Use `hyalo backlinks <path>` to find all files that link to a given file (reverse link
lookup). This builds an in-memory link graph by scanning all `.md` files in the directory,
detecting both `[[wikilinks]]` and `[markdown](links)` in body content *and* in
list-valued frontmatter properties (default: `related`, `depends-on`, `supersedes`,
`superseded-by` — configurable via `[links] frontmatter_properties = [...]` in `.hyalo.toml`).
The file can be passed positionally or with `--file`.

```bash
# Which files reference iteration-37?
hyalo backlinks iterations/iteration-37-bulk-mutations.md

# JSON output for programmatic use
hyalo backlinks iterations/iteration-37-bulk-mutations.md --format json
```

Supports `--format text` (compact), `--format json`, and `--limit N` (default: 50,
use `--limit 0` for all). Format auto-detects when not passed. Useful for impact
analysis (what depends on this file?), finding orphan pages, and navigating link
structure.

## Default output limits

List commands (`find`, `lint`, `tags summary`, `properties summary`, `backlinks`) return at
most **50 results** by default to avoid flooding the context window. When results are truncated,
output shows "showing N of M matches" and a hint to get all results.

- `--limit N` — override the default (e.g. `--limit 20` for fewer, `--limit 200` for more)
- `--limit 0` — unlimited output (returns everything)
- `--count` — just get the total count without any results

The default can be changed in `.hyalo.toml`:
```toml
default_limit = 100   # 0 = unlimited
```

## Snapshot index — ALWAYS create for vaults with 500+ files

**For any vault with more than ~500 files, ALWAYS create a snapshot index before running
queries.** The index makes property/tag queries 10-15x faster (e.g. ~80ms vs ~1.5s on a
14K-file vault). Without it, every query scans every file from disk.

**Rule of thumb:** run `hyalo summary` first. If it reports more than 500 files,
immediately create an index before proceeding with any analysis.

```bash
# Step 1: Check vault size
hyalo summary

# Step 2: Create index if >500 files (one scan, reused by all subsequent queries)
hyalo create-index

# Step 3: Use --index on ALL subsequent commands (defaults to .hyalo-index in vault dir)
hyalo find --property status=in-progress --index
hyalo summary --index
hyalo tags summary --index
hyalo backlinks some-note.md --index

# Mutations also work with --index — they patch the index after each write
hyalo set note.md --property status=completed --index
hyalo task toggle note.md --line 5 --index

# Drop the index when done
hyalo drop-index
```

The index is **ephemeral** — create it, use it, drop it within the same session. Never persist
it across sessions.

**Index-aware mutations:** all mutation commands (`set`, `remove`, `append`, `task`, `mv`,
`tags rename`, `properties rename`) support `--index`. They still read/write individual files
on disk, but after each mutation they patch the in-memory index entry and save the snapshot
back — keeping it current for subsequent queries. This is safe as long as **no external tool
modifies files in the vault** while the index is active. If only hyalo touches the files,
the index stays consistent across interleaved reads and writes.
