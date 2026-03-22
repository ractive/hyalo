---
title: "Deep Dive: Unified find command as the query layer"
type: research
date: 2026-03-22
tags:
  - research
  - cli
  - search
  - llm
  - ux
status: completed
---

# Deep Dive: Unified `find` command as the query layer

> **Note:** This is the exploration/discussion history. The decided design is captured in [[iterations/iteration-12-cli-redesign]]. This document is kept for rationale and decision context.

## The Problem Restated

Hyalo's read-side commands fall into two categories:

| Category | Commands | Returns |
|----------|----------|---------|
| **Discovery** — which files match? | `property find`, `tag find` | file list |
| **Inspection** — what's inside? | `outline`, `tasks`, `properties list/summary`, `tags list/summary`, `summary`, `links` | structured detail |

Discovery is currently **one-dimensional**: you can filter by *one* property value OR *one* tag, but not both. There's no body content search at all. An LLM agent needing "files tagged `iteration` with `status: in-progress` mentioning `rayon`" must chain 3 commands and intersect results via jq/shell scripting.

The question: can a single `find` command unify all discovery, add content search, and optionally absorb parts of inspection — while staying composable with the existing commands?

## What Existing Commands Would It Replace?

### Commands `find` directly replaces

| Current command | Equivalent `find` | Verdict |
|---|---|---|
| `property find --name status --value draft` | `find --property status=draft` | Strict superset. `find` can add more filters. |
| `property find --name status` (existence check) | `find --property status` | Same. |
| `tag find --name backlog` | `find --tag backlog` | Same. Both support nested matching. |
| `tag find --name backlog --glob 'iterations/*.md'` | `find --tag backlog --glob 'iterations/*.md'` | Same. |

These two subcommands (`property find` and `tag find`) become special cases of `find` with a single filter. They could remain as aliases/shortcuts, or be deprecated once `find` is solid.

### Commands `find` partially replaces

| Current command | `find` equivalent | What changes |
|---|---|---|
| `tasks --todo` | `find --has-todo --show tasks` | `find` adds the ability to combine with metadata filters. But `tasks` has richer task-specific output. |
| `tasks --todo --glob 'iterations/*.md'` | `find --has-todo --glob 'iterations/*.md' --show tasks` | Same observation. |
| `outline --glob 'research/*.md'` | `find --glob 'research/*.md' --show outline` | `find` makes outline a projection, not the entry point. |

The `tasks` and `outline` commands stay — they're *inspection* tools, not discovery tools. But `find --show tasks` can produce the same output when you need it combined with other filters.

### Commands `find` does NOT replace

| Command | Reason |
|---|---|
| `property read/set/remove` | Single-file mutation, not discovery |
| `property add-to-list / remove-from-list` | Mutation |
| `tag add / remove` | Mutation |
| `task read / toggle / set-status` | Single-task mutation |
| `links` | Single-file inspection (but `find` could add `--has-unresolved-links`) |
| `summary` | Vault-wide aggregation, different purpose |
| `properties summary` / `tags summary` | Schema discovery, different purpose |
| `properties list` / `tags list` | Per-file detail dump, more inspection than discovery |

## Design

### Name: `find` not `search` or `query`

- `search` implies text search — but the command does more than text search.
- `query` implies a query language / DSL — overloaded.
- `find` is universally understood. Unix `find` finds files by criteria. That's exactly what this does: find markdown files matching a combination of criteria.

Counter-argument: the iteration plan already calls it `search` (iteration 8). We could keep `search` if we want to match the plan. But `find` is more accurate for what it does — it finds files, optionally with content matching as one of many filter dimensions.

### Syntax

```
hyalo find [PATTERN] [--tag NAME]... [--property EXPR]... [--glob PATTERN]
           [--has-todo | --has-done | --has-tasks]
           [--has-links | --has-unresolved-links]
           [--show FIELD]... [--section HEADING]
           [--sort FIELD] [--limit N]
```

**Content search** is the positional argument — the most common use case gets the shortest syntax:

```sh
hyalo find "retry backoff"              # body content search
hyalo find "retry" --tag research       # content + tag
hyalo find --property status=planned    # metadata only, no content search
```

**Metadata filters** are repeatable flags, AND'd together:

```sh
# Files tagged 'iteration' with status 'in-progress'
hyalo find --tag iteration --property status=in-progress

# Files in research/ tagged 'cli' that mention 'parallelization'
hyalo find "parallelization" --tag cli --glob 'research/*.md'

# Files with unresolved links (broken wiki references)
hyalo find --has-unresolved-links
```

**Property filters** are repeatable, AND'd, and use `=` as the name/value delimiter:

```sh
--property status=planned             # equality
--property status=planned --property topic=refactoring  # multiple properties (AND)
--property status!=superseded         # not equal
--property priority>=3                # numeric comparison
--property status                     # existence (no = means "has this property")
```

**Why `--property key=value` is safe:** Obsidian property names are restricted to alphanumeric characters, spaces, hyphens, and underscores in practice. They never contain `=`, `!`, `>`, or `<`. So `--property status=planned` is unambiguous: split on the first `=`, everything before is the name (plus optional operator suffix), everything after is the value — which can contain anything, including `=` itself.

Parsing rules:
1. If no `=` present → existence check (`--property status`)
2. Scan the name portion (before first `=`) for a trailing operator (`!=`, `>=`, `<=`, `>`, `<`)
3. If operator found → split name from operator, value is everything after `=`
4. If no operator → default to equality, value is everything after the first `=`

Examples with special characters in values:

```sh
--property title=E=mc2               # name: "title", value: "E=mc2" (split on FIRST =)
--property 'title=He said "hello"'   # name: "title", value: He said "hello"
--property 'url=https://example.com' # name: "url", value: https://example.com
--property status!=in-progress       # name: "status", op: !=, value: "in-progress"
--property priority>=3               # name: "priority", op: >=, value: "3"
```

Shell quoting is only needed when the value contains spaces or shell metacharacters — standard shell rules, no hyalo-specific encoding. The `>` and `<` operators are inside a single argument so they don't trigger shell redirection.

Operator summary:

| Syntax | Meaning |
|---|---|
| `name=value` | equals |
| `name!=value` | not equals |
| `name>value` | greater than (numeric/date) |
| `name>=value` | greater or equal |
| `name<value` | less than |
| `name<=value` | less than or equal |
| `name` (no `=`) | property exists, any value |

**Task filter** uses `--task STATUS` — one flag for any task status:

```sh
hyalo find --task todo                   # files with incomplete tasks (status = space)
hyalo find --task done                   # files with completed tasks (status = x/X)
hyalo find --task /                      # files with in-progress tasks
hyalo find --task '?'                    # files with question-mark tasks
hyalo find --task any                    # files with any tasks at all
hyalo find --task todo --tag iteration   # incomplete tasks in iteration files
```

Special keywords: `todo` (status = space), `done` (status = x/X), `any` (any task exists). Everything else is a literal status character. This naturally supports every custom task status Obsidian allows without adding new flags.

### Output: Files as primary unit, projections via `--show`

Default output is a **file list with match context** — just enough to understand *why* each file matched:

```json
[
  {
    "file": "research/retry-strategy.md",
    "matches": {
      "content": [
        {"line": 23, "section": "## Exponential Backoff", "text": "We chose retry backoff with jitter..."}
      ],
      "property": {"status": "completed"},
      "tag": "research"
    }
  }
]
```

The `matches` object only includes the dimensions that were filtered on. If you searched by tag only, there's no `content` key. This keeps the output minimal by default.

**`--show` adds projections** — extra data per file:

```sh
hyalo find --tag iteration --show properties --show tasks
```

```json
[
  {
    "file": "iterations/iteration-10-comment-block-handling.md",
    "matches": {"tag": "iteration"},
    "properties": [
      {"name": "status", "type": "text", "value": "completed"},
      {"name": "date", "type": "date", "value": "2026-03-21"}
    ],
    "tasks": [
      {"line": 15, "status": "x", "text": "Implement scanner changes", "done": true},
      {"line": 16, "status": "x", "text": "Add tests", "done": true}
    ]
  }
]
```

Available `--show` values:

| Value | What it adds per file |
|---|---|
| `properties` | Full frontmatter key-value pairs |
| `tags` | Tag array |
| `tasks` | Task list (respects `--has-todo`/`--has-done` as sub-filter) |
| `outline` | Section tree (headings, line numbers, task counts) |
| `links` | Outgoing links with resolution status |
| `all` | Everything above |

This is the key insight: **`--show` turns `find` into a discovery+inspection hybrid without bloating the default output.** An LLM agent that just wants file paths uses plain `find`. One that wants to avoid follow-up calls uses `--show`.

### Section extraction: `--section`

```sh
# Extract a specific section's body content
hyalo find --file decision-log.md --section "DEC-013"
```

```json
[
  {
    "file": "decision-log.md",
    "section": {
      "heading": "## DEC-013 — Defer backlinks to indexing",
      "level": 2,
      "line_start": 145,
      "line_end": 162,
      "content": "Backlinks require scanning all files...\n\nDeferred until SQLite indexing..."
    }
  }
]
```

`--section` can combine with content search:

```sh
# Find the section about "retry" in all research docs
hyalo find "retry" --glob 'research/*.md' --section "retry"
```

This finds files containing "retry" and then extracts sections whose headings match "retry". The `--section` acts as both a heading filter and a content extractor.

Section matching is **case-insensitive substring** on heading text. For exact matches, quote the full heading.

### Sorting and limiting

```sh
hyalo find --tag backlog --sort modified     # most recently modified first
hyalo find --tag backlog --sort file         # alphabetical (default)
hyalo find --property status=draft --limit 5 # first 5 results
```

`--sort` options: `file` (default, alphabetical), `modified` (mtime descending), `matches` (most content matches first, only meaningful with content search).

### Text output

```sh
$ hyalo find "retry" --tag research --format text
research/retry-strategy.md
  tag: research
  L23 [## Exponential Backoff]: We chose retry backoff with jitter...
  L41 [## Exponential Backoff]: ...the backoff ceiling is 30s...

research/error-handling.md
  tag: research
  L8  [## Retry Logic]: Default retry with exponential backoff...
```

With `--show tasks --format text`:
```sh
$ hyalo find --has-todo --tag iteration --property status!=superseded --show tasks --format text
iterations/iteration-10-comment-block-handling.md
  [x] L15: Implement scanner changes
  [x] L16: Add tests
  [ ] L17: Update documentation

iterations/iteration-09-tasks-and-summary.md
  [x] L12: Summary command
  [ ] L13: Task toggle
```

This directly solves ISSUE-1 from dogfooding (noisy empty-file output) because `find` only returns matching files.

## How It Subsumes Existing Commands

`find` replaces all per-file reading commands. See the "Full Command Redesign" section below for the complete mapping.

## Design Decisions to Make

### D1: `--property key=value` parsing is safe — Obsidian naming rules guarantee it

**Decision: Use `=` as the name/value delimiter in `--property` flags.**

This was initially a concern: what if a property name contains `=`, `!`, `>`, or `<`? Parsing `--property equation=E=mc2` would be ambiguous.

**Investigation:** Obsidian's property name rules resolve this. Property names in practice are limited to alphanumeric characters, spaces, hyphens, and underscores. The official Obsidian docs say "choose anything you like" but since properties are YAML keys, characters like `:` or `=` would need YAML quoting and are essentially never used. Community practice confirms: no one uses `=`, `!`, `>`, or `<` in property names.

Tag names are even more restricted: only `[a-zA-Z0-9_/-]` (letters, numbers, underscore, hyphen, forward slash for nesting).

**Therefore:** Splitting on the first `=` is always unambiguous. The operator characters (`!`, `>`, `<`) can safely be detected as trailing suffixes on the name portion because they never appear in real property names.

**References:**
- [Obsidian Properties docs](https://help.obsidian.md/properties)
- [Obsidian Tags docs](https://help.obsidian.md/tags)
- Tag validation regex from Obsidian source blocks all common punctuation including `=`, `>`, `<`, `!`

### D2: Content search — regex or substring?

Options:
1. **Substring** (case-insensitive by default, `--case-sensitive` flag) — simple, no escaping issues
2. **Regex** — more powerful, but quoting hell in shell
3. **Substring default, `--regex` opt-in** — best of both worlds

Recommendation: Option 3. Substring covers 90% of use cases. An LLM agent rarely needs regex; it just wants to find files mentioning a term.

### D3: Content search scope — body only, or frontmatter too?

Recommendation: **Body only by default.** Frontmatter is already queryable via `--property` and `--tag`. Searching frontmatter as text would produce confusing duplicates. If needed, a `--search-frontmatter` flag can be added later.

### D4: How does `--section` interact with `--show outline`?

If both are given: `--section` filters which sections are included in the outline projection. If only `--section`: it extracts body content of matching sections.

### D5: Should `find` with zero filters return all files?

Options:
1. **Yes** — consistent with `properties list` (no filters = all files). `hyalo find` = file list of entire vault.
2. **Error** — require at least one filter to prevent accidental dumps.
3. **Yes, but warn** — return all files with a hint that filters are available.

Recommendation: Option 1. Consistency matters. `hyalo find --show properties` becomes a synonym for `hyalo properties list`. `hyalo find --limit 10 --sort modified` becomes "10 most recently modified files" — useful on its own.

### D6: Performance — does content search need indexing?

For vaults under ~1000 files: **no**. Streaming line-by-line scan (hyalo's existing `FileVisitor` pattern) handles this fine. The scanner already reads files in a single pass; adding substring matching to `on_body_line` is cheap.

For large vaults: content search benefits from indexing. But SQLite indexing is already a planned iteration. `find` should work without an index (scan on every call) and transparently use one when available.

### D7: Name — `find` vs `search`?

Arguments for `find`:
- Matches the mental model (find files by criteria)
- Doesn't overload with "full-text search" expectations
- Short, familiar

Arguments for `search`:
- Already named `search` in the iteration plan and backlog
- `find` conflicts with the existing `property find` / `tag find` subcommand names (though different command level)
- The iteration plan calls it "Search" with content search as the headline feature

Arguments for `query`:
- Precise — it is a query
- But overloaded (database queries, query languages)

Recommendation: Either `find` or `search` works. The key is that it's a top-level command, not a subcommand of anything.

## Concrete Workflows — Before and After

### "Files tagged backlog with status planned"

**Before (3 commands + scripting):**
```sh
hyalo tag find --name backlog --jq '.files[]' > /tmp/a.txt
hyalo property find --name status --value planned --jq '.files[]' > /tmp/b.txt
comm -12 <(sort /tmp/a.txt) <(sort /tmp/b.txt)
```

**After (1 command):**
```sh
hyalo find --tag backlog --property status=planned
```

### "Incomplete tasks in active iterations"

**Before (impossible without scripting):**
```sh
hyalo property find --name status --value in-progress --jq '.files[]' \
  | xargs -I{} hyalo tasks --todo --file {}
```

**After (1 command):**
```sh
hyalo find --task todo --property status=in-progress
```

### "Find where we discussed SQLite"

**Before (2 tool calls for an LLM agent):**
```
1. grep -r "SQLite" hyalo-knowledgebase/          → find files
2. Read the matching files                         → read content
```

**After (1 command):**
```sh
hyalo find "SQLite" --format text
```

### "What's in the backlog about search?"

**Before (2 commands):**
```sh
hyalo tag find --name backlog --jq '.files[]'
# then grep each file for "search" or read each one
```

**After (1 command):**
```sh
hyalo find "search" --tag backlog --format text
```

### "Orient me in this vault"

**Before (3-5 commands for an LLM agent):**
```sh
hyalo summary --format text
hyalo tags summary --format text
hyalo properties summary --format text
hyalo tasks --todo --format text
```

**After (2 commands):**
```sh
hyalo summary --format text                                                      # vault overview
hyalo find --task todo --property status!=superseded --format text  # actionable work
```

`summary` stays as the vault-level aggregate. `find` replaces the multi-command drill-down.

## Full Command Redesign (Decided)

### The File Object

Every markdown file is a structured object with these fields:

```json
{
  "file": "iterations/iteration-10.md",
  "modified": "2026-03-21T14:23:45Z",
  "properties": [{"name": "status", "type": "text", "value": "completed"}, ...],
  "tags": ["iteration", "cli"],
  "sections": [{"level": 1, "heading": "Title", "line": 5, "links": [...],
                 "tasks": {"total": 3, "done": 3}, "code_blocks": ["rust"]}],
  "tasks": [{"line": 15, "section": "## Implementation", "status": "x",
             "text": "Implement scanner", "done": true}, ...],
  "links": [{"target": "decision-log", "path": "decision-log.md", "label": null}, ...],
  "matches": [{"line": 23, "section": "## Design", "text": "...matched line..."}]
}
```

`sections` is what `outline` currently returns. `matches` only appears when content search is active.

### `--fields` flag

Controls which fields are returned per file. Comma-separated, no spaces.

```sh
hyalo find --tag backlog --fields tasks             # only tasks
hyalo find --tag backlog --fields tasks,sections    # tasks and sections
hyalo find --tag backlog --fields properties,tags   # only frontmatter
```

**Default: all fields.** When `--fields` is omitted, `find` returns the complete file object. This is the simplest mental model — you always get everything, use `--jq` or `--fields` to narrow down.

Implementation note: the default should be easy to change later (e.g., to frontmatter-only) if performance becomes an issue on large vaults. Internally, "no `--fields` flag" maps to a configurable default set, not a hardcoded "all".

### `--task STATUS` filter

A unified task filter that replaces the old `--has-todo`/`--has-done`/`--has-tasks` flags:

```sh
hyalo find --task todo     # files with incomplete tasks (status = space)
hyalo find --task done     # files with completed tasks (status = x/X)
hyalo find --task /        # files with in-progress tasks
hyalo find --task '?'      # files with question-mark tasks
hyalo find --task any      # files with any tasks at all
```

Special keywords: `todo`, `done`, `any`. Everything else is a literal status character matching Obsidian's custom task statuses.

### Complete Command Mapping: Current → Proposed

#### Reading — `find` absorbs all per-file queries

| Current command | Proposed equivalent |
|---|---|
| `property find --name status --value draft` | `find --property status=draft` |
| `property find --name status` | `find --property status` |
| `tag find --name backlog` | `find --tag backlog` |
| `properties list --file F` | `find --file F` (properties included by default) |
| `properties list --glob G` | `find --glob G` |
| `tags list --file F` | `find --file F` (tags included by default) |
| `tags list --glob G` | `find --glob G` |
| `property read --name N --file F` | `find --file F --jq '.[0].properties'` |
| `outline --file F` | `find --file F` (sections included by default) |
| `outline --glob G` | `find --glob G` |
| `tasks --todo` | `find --task todo` |
| `tasks --todo --glob G` | `find --task todo --glob G` |
| `tasks --done` | `find --task done` |
| `tasks --status /` | `find --task /` |
| `task read --file F --line N` | `find --file F --jq '.[0].tasks[]  \| select(.line == N)'` |
| `links --file F` | `find --file F` (links included by default) |
| `links --file F --unresolved` | `find --file F --jq '.[0].links[] \| select(.path == null)'` |
| *(new)* content search | `find "search term"` |
| *(new)* combined filters | `find --tag backlog --property status=planned` |
| *(new)* multi-property filter | `find --property status=draft --property type=iteration` |
| *(new)* task + metadata filter | `find --task todo --property status!=superseded` |

#### Reading — aggregate commands become subcommand-free

| Current command | Proposed equivalent |
|---|---|
| `properties summary` | `properties` (no subcommand) |
| `properties list` | *(removed — use `find`)* |
| `tags summary` | `tags` (no subcommand) |
| `tags list` | *(removed — use `find`)* |
| `summary` | `summary` (unchanged, reuses aggregate models internally) |

`tasks` and `links` aggregate commands are deferred for now. Their data is available via `summary`.

Aggregate commands accept `--glob` to scope but NOT `--file` (aggregating one file is pointless — use `find --file F` instead).

#### Writing — unified `set`/`remove`/`append`

| Current command | Proposed equivalent |
|---|---|
| `property set --name status --value done --file F` | `set --property status=done --file F` |
| `property remove --name status --file F` | `remove --property status --file F` |
| `tag add --name rust --file F` | `set --tag rust --file F` |
| `tag add --name rust --glob G` | `set --tag rust --glob G` |
| `tag remove --name rust --file F` | `remove --tag rust --file F` |
| `tag remove --name rust --glob G` | `remove --tag rust --glob G` |
| `property add-to-list --name deps --value serde --file F` | `append --property deps=serde --file F` |
| `property remove-from-list --name deps --value serde --file F` | `remove --property deps=serde --file F` |
| `task toggle --file F --line N` | `task toggle --file F --line N` (unchanged) |
| `task set-status --file F --line N --status /` | `task set-status --file F --line N --status /` (unchanged) |

**`remove` semantics:** `remove --property deps=serde` removes value "serde" from list property "deps", OR removes the property "deps" entirely if its value equals "serde". `remove --property deps` removes the property regardless of content. The presence of `=value` disambiguates. Good error messages needed (e.g., "property 'status' is not a list — removed property because value matched").

Task mutations stay as their own `task` command — they're line-number-based body edits, fundamentally different from frontmatter mutations.

### The Complete Proposed CLI

```
READING (per-file)
  find       [PATTERN] [--property K=V]... [--tag T]... [--task STATUS]
             [--glob G | --file F] [--fields F1,F2,...] [--sort S] [--limit N]
             → array of file objects (always an array, even for --file)

READING (aggregates)
  properties [--glob G]         → unique names, types, counts
  tags       [--glob G]         → unique tags with counts
  summary    [--glob G]         → combined vault overview

WRITING (frontmatter)
  set        --property K=V | --tag T   --file F | --glob G
  remove     --property K[=V] | --tag T  --file F | --glob G
  append     --property K=V              --file F | --glob G

WRITING (tasks)
  task toggle     --file F --line N
  task set-status --file F --line N --status C

GLOBAL FLAGS
  --dir DIR          Root directory
  --format json|text
  --jq FILTER
  --hints
```

**7 commands** total (down from 11 commands with ~20 subcommands):
- `find` — the one query command (replaces `property find`, `tag find`, `properties list`, `tags list`, `outline`, `tasks` listing, `links` inspection)
- `properties`, `tags`, `summary` — aggregates
- `set`, `remove`, `append` — frontmatter mutations
- `task` — task-specific mutations (toggle, set-status)

### What This Eliminates

Commands fully removed:
- `property` (with 6 subcommands: read, find, set, remove, add-to-list, remove-from-list)
- `tag` (with 3 subcommands: find, add, remove)
- `outline` (absorbed into `find` — sections field)
- `tasks` (listing mode absorbed into `find --task`; aggregate deferred)
- `links` (per-file absorbed into `find`; aggregate deferred)

Subcommands removed from surviving commands:
- `properties summary` / `properties list` → just `properties`
- `tags summary` / `tags list` → just `tags`

### Backwards Compatibility

This is a breaking redesign. Given hyalo is pre-1.0 with a small user base, a clean break is pragmatic. Ship as v2.0 (or just the next version if we stay pre-1.0).

## Implementation Sketch

`find` reuses existing infrastructure:

1. **File collection**: `collect_files()` already handles `--glob` / `--file` / default
2. **Frontmatter parsing**: Existing `parse_frontmatter()` for property/tag filters
3. **Content scanning**: New `ContentSearchVisitor` implementing `FileVisitor`, records matching lines + their enclosing section heading
4. **`--fields` driven visitors**: `--fields` determines which visitors are attached to the multi-visitor scanner. No `--fields tasks` = no task visitor = skip task parsing. When `--fields` is omitted (default = all), all visitors run.
5. **Output**: New `FileObject` struct in `types.rs`, reusing `PropertyInfo`, `FileTags`, `TaskInfo`, `SectionInfo`, `LinkInfo`. Fields are `Option<Vec<T>>` — `None` when not requested via `--fields`, serialized as absent (not null).

The multi-visitor pattern means a single file pass can simultaneously:
- Check frontmatter filters (early exit if no match)
- Search body content
- Build sections (if fields include `sections`)
- Collect tasks (if fields include `tasks` or `--task` filter is active)
- Collect links (if fields include `links`)

Two-phase execution for performance:
1. **Filter phase**: scan all candidate files with minimal visitors (frontmatter for metadata filters, body for content search / `--task`)
2. **Output phase**: for matching files, run all requested field visitors (may be the same pass if the filter already needed body access)

The default-all-fields behavior is controlled by a single constant/config point, making it trivial to change the default to frontmatter-only later if performance demands it.

## Open Questions

1. **Should content matches show surrounding context lines?** (like grep's `-C` flag). Probably yes: `--context N` with default 0.
2. **Should `find` support OR logic?** Start with AND only. OR is rare and can be handled by running `find` twice.
3. **`matches` field**: only present when content search is active. It's a query annotation, not a file property.
4. **`find` always returns an array**, even with `--file`. Consistent shape. Breaking from current bare-object convention but cleaner for consumers.
5. **Name: `find` vs `search`?** See D7. Both work. Decision deferred to implementation.
6. **`--fields sections`**: Not deferred — sections (heading structure, task counts, code blocks, links per section) is a standard field on the file object, powered by the existing `OutlineVisitor`. This replaces `outline`.
7. **Section-level read/write operations**: Deferred to a future iteration. The idea: treat sections (identified by heading) as addressable units for both reading and writing, giving an LLM agent structured access to markdown body content without needing line numbers or full-file reads.

   **Reading:**
   ```sh
   hyalo read --file F --section "Design Decisions"    # extract section body as markdown
   hyalo read --file F --section "Design Decisions" --format json  # structured: heading, line range, content
   ```

   **Writing:**
   ```sh
   hyalo set --section "Design Decisions" --content "New text..." --file F      # replace section body
   hyalo append --section "Design Decisions" --content "Another paragraph" --file F  # append to section
   hyalo remove --section "Design Decisions" --file F                           # remove entire section
   ```

   This extends the `set`/`remove`/`append` pattern from frontmatter to body content. Sections become a first-class addressable unit alongside properties and tags. The `--section` flag identifies the target by heading text (case-insensitive substring match), and the `OutlineVisitor` already knows the line boundaries.

   **Why this matters for LLM agents:** Today, modifying a section requires: (1) `outline` to find the heading, (2) `Read` to get line numbers, (3) `Edit` with exact string matching. With section-level operations, it's one call. This is especially powerful combined with `find` — discover a file via metadata/content search, then surgically modify a specific section.
