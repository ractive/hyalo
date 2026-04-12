---
title: "Iteration 105 — Summary Redesign: Compact Output + Find Filters"
type: iteration
date: 2026-04-12
tags:
  - iteration
  - summary
  - ux
  - llm
  - cli
status: in-progress
branch: iter-105/summary-redesign
---

# Iteration 105 — Summary Redesign: Compact Output + Find Filters

## Goal

Make `hyalo summary` a compact, fixed-size orientation command (~20-30 lines regardless of vault size) and add `find --orphan` / `find --dead-end` filter flags so file lists are accessible through `find` with full sorting, limiting, and field control.

## Motivation

Today `hyalo summary` dumps every orphan, dead-end, broken-link source, and status-grouped file path inline. On large vaults this produces enormous output (18k lines for MDN, 5k for GitHub docs) where 99%+ is file path listings that nobody reads. Even on hyalo's own 220-file KB, orphans + dead-ends account for 147 of 178 text lines.

The summary command should answer: "What is this KB, what state is it in, and what needs attention?" — not serve as a file listing tool. File lists belong in `find`, which already has `--sort`, `--limit`, `--fields`, `--jq`, `--glob`, and `--format`.

This is the entry-point command for AI agents (and `/hyalo-tidy`). It must be short enough to fit in a system prompt without truncation.

## Design

### Summary output changes (both JSON and text)

**Remove all file lists from summary:**
- `orphans.files` → remove (keep `orphans.total` as `orphans`)
- `dead_ends.files` → remove (keep `dead_ends.total` as `dead_ends`)
- `links.broken_links` → remove (keep `links.total` and `links.broken`)
- `status[].files` → remove (keep `status[].value` and add `status[].count`)
- `files.by_directory` → collapse to depth-1 only, show top-level dirs with counts

**Add inline top-N for properties and tags:**
- Properties: show top 5-7 by count, with count, inline in text. JSON: already has full list, just keep it.
- Tags: show top 5-7 by count, inline in text. JSON: already has full list, just keep it.

**Status: counts instead of file lists:**
- Current: `{value: "completed", files: [183 paths]}`
- New: `{value: "completed", count: 183}`
- For actionable statuses (planned, in-progress, active) — the counts are enough. The agent can drill down with `hyalo find --property status=planned`.

**Broken links: count only, no details:**
- Current: `{total: 166, broken: 5, broken_links: [{source, line, target}, ...]}`
- New: `{total: 166, broken: 5}`
- Drill-down: `hyalo find --broken-links` or `hyalo links fix`

**Directory breakdown: depth-1 only:**
- Current: every nested directory with count
- New: top-level directories only (depth 0-1), sorted by count descending
- The existing `--depth` flag stays for override

### New find filter flags

**`find --orphan`** — return files with zero inbound links (no other file links to them).

Parallels `--broken-links`. Requires link graph computation (same as `--fields backlinks` but filtered to count == 0). Auto-includes `backlinks` field in output so the agent can see the confirmation.

**`find --dead-end`** — return files with inbound links but zero outbound links.

Excludes orphans (which have neither). Auto-includes `links` field.

Both compose with all existing find flags: `--property`, `--tag`, `--glob`, `--sort`, `--limit`, `--fields`, `--format`, `--jq`.

### Hints update

Summary hints should point to the new drill-down commands:
- `hyalo find --orphan` — N orphan files
- `hyalo find --dead-end` — N dead-end files
- `hyalo find --broken-links` — N files with broken links
- `hyalo find --property status=planned` — N planned items
- `hyalo find --task todo` — N open tasks

### Target text output (~20-30 lines)

```
Files: 220
Directories: iterations/ (101), backlog/ (84), research/ (26), dogfood-results/ (7), . (2)
Properties: 12 — status (209), title (215), date (214), type (209), tags (168), ...
Tags: 107 — iteration (83), cli (71), ux (70), backlog (68), performance (32), ...
Tasks: 1667/1804
Links: 166 total, 5 broken
Orphans: 74
Dead-ends: 72
Status: completed (183), planned (4), active (3), wont-do (7), shelved (3), ...
Recent: iteration-102-frontmatter-types-schema.md, karpathy-llm-wiki.md, ...

  -> hyalo find --orphan                     # 74 orphan files
  -> hyalo find --dead-end                   # 72 dead-end files
  -> hyalo find --broken-links               # 5 files with broken links
  -> hyalo find --property status=planned    # 4 planned items
  -> hyalo find --task todo                  # Find files with open tasks
```

### Target JSON output

```json
{
  "results": {
    "files": { "total": 220, "directories": [{"directory": "iterations", "count": 101}, ...] },
    "properties": [{"name": "status", "type": "text", "count": 209}, ...],
    "tags": {"total": 107, "tags": [{"name": "iteration", "count": 83}, ...]},
    "tasks": {"total": 1804, "done": 1667},
    "links": {"total": 166, "broken": 5},
    "orphans": 74,
    "dead_ends": 72,
    "status": [{"value": "completed", "count": 183}, ...],
    "recent_files": [{"path": "...", "modified": "..."}]
  },
  "hints": [...]
}
```

## Breaking changes

The JSON schema changes:
- `orphans` changes from `{total, files}` to plain integer
- `dead_ends` changes from `{total, files}` to plain integer
- `links.broken_links` removed
- `status[].files` replaced by `status[].count`
- `files.by_directory` renamed to `files.directories`, default depth changes

Since hyalo is pre-1.0 and JSON consumers are primarily AI agents (which adapt), this is acceptable.

## Tasks

### Summary output refactoring
- [x] Change `OrphanSummary` to just a count (usize) in `VaultSummary`
- [x] Change `DeadEndSummary` to just a count (usize) in `VaultSummary`
- [x] Remove `broken_links` vec from `LinkHealthSummary`
- [x] Change `StatusGroup` to carry `count: usize` instead of `files: Vec<String>`
- [x] Collapse `files.by_directory` to depth-1 by default in summary builder
- [x] Rename `by_directory` to `directories` in `FileCounts`
- [x] Update text formatter: single-line properties/tags with top-N and counts
- [x] Update text formatter: single-line status with counts
- [x] Update text formatter: remove orphan/dead-end/broken-link file listings
- [x] Update text formatter: single-line directories (top-level only, sorted by count)
- [x] Update JSON serialization to match new schema

### Find filter flags
- [x] Add `--orphan` flag to `FindArgs`
- [x] Implement orphan filtering in find command (backlinks count == 0)
- [x] Add `--dead-end` flag to `FindArgs`
- [x] Implement dead-end filtering in find command (outbound links == 0, inbound > 0)
- [x] Auto-include relevant fields when `--orphan` or `--dead-end` is used
- [x] Handle view merging for new boolean flags (OR semantics, like `--broken-links`)

### Hints
- [x] Update summary hints to reference `find --orphan`, `find --dead-end`
- [x] Add count-aware hint text (e.g., "74 orphan files")
- [x] Suppress orphan/dead-end hints when counts are zero

### Tests
- [x] Unit tests for compact summary output (text + JSON)
- [x] Unit tests for `find --orphan` filtering
- [x] Unit tests for `find --dead-end` filtering
- [x] E2E test: summary output is compact (no file lists in either format)
- [x] E2E test: `find --orphan` returns correct files
- [x] E2E test: `find --dead-end` returns correct files
- [x] E2E test: `find --orphan --sort modified --limit 5` composes correctly
- [x] E2E test: `find --orphan --glob 'research/*'` composes correctly

### Documentation & help texts
- [x] Update `summary` command help text and long_about in args.rs
- [x] Update `find` command help text to document `--orphan` and `--dead-end`
- [x] Update help.rs overview text and example commands
- [x] Update `--jq` examples if they reference removed summary fields
- [x] Update README.md summary examples and any jq examples
- [x] Update skill template (`templates/skill-hyalo.md`) with new summary output and new find flags
- [x] Update hyalo claude rule (`templates/rule-knowledgebase.md`) if summary usage is mentioned
- [x] Update `.claude/CLAUDE.md` if it references summary output format
- [x] Update search cookbook (`recipes/search-cookbook.md`) with orphan/dead-end examples

### Quality gates
- [x] cargo fmt
- [x] cargo clippy --workspace --all-targets -- -D warnings
- [x] cargo test --workspace
- [x] Dogfood against hyalo-knowledgebase, mdn, docs, vscode-docs

## Acceptance Criteria
- [x] `hyalo summary --format text` produces ≤30 lines on any vault size
- [x] `hyalo summary --format json` contains no file-path arrays (orphans, dead-ends, broken links, status groups)
- [x] `hyalo find --orphan` returns files with zero inbound links
- [x] `hyalo find --dead-end` returns files with inbound links but zero outbound links
- [x] Both new flags compose with `--property`, `--tag`, `--glob`, `--sort`, `--limit`, `--fields`
- [x] Summary hints reference `find --orphan` and `find --dead-end` with counts
- [x] MDN vault (14k files) summary is ≤30 lines in text mode
- [x] All quality gates pass
