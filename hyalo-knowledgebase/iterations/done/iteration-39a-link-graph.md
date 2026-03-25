---
title: "Iteration 39a — Link Graph & Backlinks"
type: iteration
date: 2026-03-25
tags: [iteration, links, wikilinks, cli]
status: completed
branch: iter-39/link-graph
---

# Iteration 39a — Link Graph & Backlinks

## Goal

Build an in-memory link graph to enable vault-wide link operations, starting with backlinks. No persistent index — the graph is built on demand per invocation by scanning all `.md` files.

## Design decisions

**No SQLite index.** Reading and parsing markdown files is fast enough that an in-memory `HashMap<PathBuf, Vec<BacklinkEntry>>` built per CLI invocation is sufficient. The drawbacks of a persistent index (staleness, cache invalidation, extra dependency, file locking on Windows) outweigh the few milliseconds saved. See [[backlog/sqlite-indexing]] for the original proposal — it can be revisited if vaults grow to tens of thousands of files.

**Reuse existing scanner.** The `FileVisitor` trait and `scan_file_multi` already handle fence tracking, inline code stripping, and comment skipping. A new `LinkGraphVisitor` reuses this infrastructure rather than building a separate scanner.

**Skip frontmatter parsing for link-only scans.** Add `needs_frontmatter() -> bool` to `FileVisitor` (default `true`). When no visitor needs frontmatter, the scanner reads past `---` delimiters but skips YAML accumulation and `serde_yaml_ng` parsing.

## Backlog items

- [[backlog/backlinks]] (medium)
- [[backlog/move-rename-command]] (medium)
- [[backlog/shortest-path-link-resolution]] (medium)

## Tasks

### Scanner optimisation [2/2]
- [x] Add `needs_frontmatter() -> bool` to `FileVisitor` trait (default `true`)
- [x] When no visitor needs frontmatter, skip YAML accumulation and `serde_yaml_ng` parse (still read past `---` delimiters)

### In-memory link graph [6/6]
- [x] `LinkGraphVisitor` implementing `FileVisitor` (returns `false` from `needs_frontmatter`)
- [x] Reuse existing `extract_links_from_text` — capture `(line_num, Link)` pairs per file
- [x] `BacklinkEntry { source: PathBuf, line: usize, link: Link }` struct for reverse index entries
- [x] Build `HashMap<PathBuf, Vec<BacklinkEntry>>` mapping target → list of sources
- [x] Scan all `.md` files in vault directory
- [x] E2e tests cover graph construction

### Backlinks [5/5]
- [x] `hyalo backlinks <file>` lists files that link to the given file
- [x] Works with both `[[wikilink]]` and relative path links
- [x] Shows line number and link text for each backlink
- [x] `--format text` and `--format json` output modes
- [x] E2e tests cover backlinks

## Acceptance Criteria [3/3]

- [x] In-memory graph builds in under 1s for 1000-file vaults
- [x] Backlinks query returns correct results
- [x] All quality gates pass (fmt, clippy, tests)

## Notes

Split from original iteration 39. Move/rename command deferred to [[iterations/iteration-39b-move-command]]. Config-aware help text and shortest-path resolution moved to backlog.
