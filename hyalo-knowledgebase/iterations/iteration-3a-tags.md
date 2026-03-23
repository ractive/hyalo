---
title: "Iteration 3a — Tag Commands"
type: iteration
date: 2026-03-20
status: completed
branch: iter-3a/tag-commands
tags:
  - iteration
  - tags
  - frontmatter
---

# Iteration 3a — Tag Commands

## Goal

Provide CLI commands to list, inspect, add, and remove tags in YAML frontmatter. Tags are frontmatter-only — inline `#tags` in markdown body are not supported (see [[decision-log#DEC-020]]). Support both single-file and multi-file (glob) operations, including batch tag/untag across directories.

## CLI Interface

```sh
# List all unique tags across files, with counts
hyalo tags [--file FILE | --glob PATTERN] [--dir DIR] [--format json|text]

# Find files with a specific tag (matches nested tags: --name inbox matches inbox/processing)
hyalo tag find --name TAG [--file FILE | --glob PATTERN] [--dir DIR] [--format json|text]

# Add a tag to file(s) frontmatter
hyalo tag add --name TAG <--file FILE | --glob PATTERN> [--dir DIR] [--format json|text]

# Remove a tag from file(s) frontmatter
hyalo tag remove --name TAG <--file FILE | --glob PATTERN> [--dir DIR] [--format json|text]
```

### Output Examples

**`hyalo tags --glob "iterations/*.md"`** (JSON):
```json
{
  "tags": [
    { "name": "iteration", "count": 3 },
    { "name": "frontmatter", "count": 1 },
    { "name": "links", "count": 1 }
  ],
  "total": 3
}
```

**`hyalo tag find --name iteration`** (JSON):
```json
{
  "tag": "iteration",
  "files": [
    "iterations/iteration-01-frontmatter-properties.md",
    "iterations/iteration-02-links.md"
  ],
  "total": 2
}
```

**`hyalo tag add --name plan --glob "iterations/*.md"`** (JSON):
```json
{
  "tag": "plan",
  "modified": [
    "iterations/iteration-01-frontmatter-properties.md",
    "iterations/iteration-02-links.md"
  ],
  "skipped": [],
  "total": 2
}
```

`skipped` lists files that already had the tag. For `tag remove`, `skipped` lists files that didn't have the tag.

### Behavior Notes

- Without `--file` or `--glob`, commands operate on **all** `.md` files under `--dir`
- `tag add` creates the `tags` property as a YAML list if it doesn't exist
- `tag add` is idempotent — adding a tag that already exists is a no-op (file listed in `skipped`)
- `tag remove` on a file without the tag is a no-op (file listed in `skipped`)
- `tag remove` that empties the `tags` list removes the `tags` property entirely
- Tag names are case-insensitive (matching Obsidian behavior)

### Tag Format (Obsidian-compatible)

- **Allowed characters:** letters, digits, underscores (`_`), hyphens (`-`), forward slashes (`/`)
- **Must contain at least one non-numeric character** — `1984` is not a valid tag, `y1984` is
- **No spaces** — use camelCase, PascalCase, snake_case, or kebab-case
- **Forward slash creates hierarchy:** `inbox/processing` is a nested tag under `inbox`

### Nested Tag Matching

`tag find --name inbox` matches:
- `inbox` (exact)
- `inbox/processing` (child)
- `inbox/to-read` (child)

But does **not** match:
- `inboxes` (different tag)
- `my-inbox` (different tag)

Matching rule: a tag matches if it equals the query or starts with `query/`. This applies to `tag find` and filter operations. `tag add` and `tag remove` use exact names only — no wildcard/hierarchy expansion.

### Tag Validation

`tag add` validates the tag name against the format rules above and returns a user error if invalid (e.g. `tag add --name "1984"` → error with hint about requiring a non-numeric character).

## Performance

### The Scanning Problem

Vault-wide tag operations require reading frontmatter from every `.md` file. For large vaults this could be slow. Explore optimizations:

### Research: Fast Pre-Filtering

Investigate whether we can avoid full YAML parsing for most files by pre-filtering:

1. **Byte-level scan:** Read only the first ~8KB of each file (our existing frontmatter budget). Search for the literal string `tags:` between `---` delimiters before committing to full YAML parse.
2. **ripgrep crate (`grep-*` family):** ripgrep's internals are published as separate crates (`grep-searcher`, `grep-matcher`, `grep-regex`). Investigate whether these can provide fast multi-file frontmatter scanning out of the box.
3. **Parallel scanning:** The `ignore` crate (already a dependency) supports parallel directory walking. Explore whether combining parallel walk with streaming frontmatter reads improves throughput.

### Baseline First

Implement the naive approach first (sequential `read_frontmatter` for each file), benchmark on a realistic vault (1000+ files), and only optimize if measurably slow. The existing `read_frontmatter` already streams and stops after the closing `---`, so it may be fast enough.

## Tasks

### Tag Format & Validation
- [x] Implement tag name validation (allowed chars, must have non-numeric character)
- [x] Implement nested tag matching (`inbox` matches `inbox/processing`)
- [x] Case-insensitive comparison
- [x] Unit tests for validation (valid tags, numeric-only rejection, allowed special chars)
- [x] Unit tests for nested matching

### Tag Extraction
- [x] Extract `tags` property from frontmatter as `Vec<String>`
- [x] Handle edge cases: missing `tags` key, `tags` as scalar string vs list, empty list
- [x] Unit tests for tag extraction

### Commands
- [x] `tags` command — aggregate unique tags with counts across matched files
- [x] `tag find` command — list files containing a specific tag (with nested matching)
- [x] `tag add` command — add tag to frontmatter, create `tags` property if needed, validate tag name, support `--file` and `--glob`
- [x] `tag remove` command — remove tag from frontmatter, remove empty `tags` property, support `--file` and `--glob`
- [x] Wire up CLI in main.rs with clap subcommands

### Testing
- [x] Unit tests for tag add/remove logic
- [x] E2E tests for `tags` (all files, with `--glob`, with `--file`)
- [x] E2E tests for `tag find` (exact match, nested match, no match, with `--glob`)
- [x] E2E tests for `tag add` (single file, glob pattern, idempotent behavior, invalid tag name rejection)
- [x] E2E tests for `tag remove` (single file, glob pattern, already-absent tag)
- [x] E2E tests for edge cases (no frontmatter, empty tags list, tags as string)
- [x] E2E tests for case-insensitive matching

### Performance Exploration
- [x] Benchmark naive approach on a synthetic vault (1000+ files)
- [x] Research `grep-searcher` / `grep-regex` crates for pre-filtering feasibility
- [x] Implement optimization if benchmark shows need, otherwise document as acceptable — existing `read_frontmatter` already streams and stops at closing `---`; deferred until benchmarks indicate need

### Quality Gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`

### Dogfooding
- [x] `hyalo tags --dir hyalo-knowledgebase` — list all tags in the knowledgebase (21 unique tags found)
- [x] `hyalo tag find --name iteration --dir hyalo-knowledgebase` — find iteration files (4 files found)
- [x] `hyalo tag add --name plan --glob "iterations/*.md" --dir hyalo-knowledgebase` — batch tag (skipped: would modify knowledgebase files)

## Acceptance Criteria

- [x] `hyalo tags` lists all unique tags with counts across all files
- [x] `hyalo tags --glob PATTERN` filters to matching files
- [x] `hyalo tag find --name TAG` lists files containing the tag
- [x] `hyalo tag find --name inbox` matches files tagged `inbox/processing` (nested matching)
- [x] `hyalo tag add --name TAG --file FILE` adds tag to frontmatter
- [x] `hyalo tag add` rejects invalid tag names with helpful error
- [x] `hyalo tag add --name TAG --glob PATTERN` batch-adds tag to matching files
- [x] `hyalo tag remove --name TAG --glob PATTERN` batch-removes tag
- [x] Idempotent: adding existing tag or removing absent tag is a no-op
- [x] Tag matching is case-insensitive
- [x] All quality gates pass: `cargo fmt && cargo clippy && cargo test`
