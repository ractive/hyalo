---
title: "Iteration 39 — Link Graph"
type: iteration
date: 2026-03-25
tags: [iteration, links, wikilinks, cli, llm]
status: planned
branch: iter-39/link-graph
---

# Iteration 39 — Link Graph

## Goal

Build an in-memory link graph to enable vault-wide link operations: backlinks, move/rename with wikilink updates, and shortest-path link resolution. No persistent index — the graph is built on demand per invocation by scanning all `.md` files.

## Design decisions

**No SQLite index.** Reading and parsing markdown files is fast enough that an in-memory `HashMap<PathBuf, Vec<BacklinkEntry>>` built per CLI invocation is sufficient. The drawbacks of a persistent index (staleness, cache invalidation, extra dependency, file locking on Windows) outweigh the few milliseconds saved. See [[backlog/sqlite-indexing]] for the original proposal — it can be revisited if vaults grow to tens of thousands of files.

**Reuse existing scanner.** The `FileVisitor` trait and `scan_file_multi` already handle fence tracking, inline code stripping, and comment skipping. A new `LinkGraphVisitor` reuses this infrastructure rather than building a separate scanner.

**Skip frontmatter parsing for link-only scans.** Add `needs_frontmatter() -> bool` to `FileVisitor` (default `true`). When no visitor needs frontmatter, the scanner reads past `---` delimiters but skips YAML accumulation and `serde_yaml_ng` parsing.

## Backlog items

- [[backlog/backlinks]] (medium)
- [[backlog/move-rename-command]] (medium)
- [[backlog/shortest-path-link-resolution]] (medium)

## Tasks

### Scanner optimisation
- [ ] Add `needs_frontmatter() -> bool` to `FileVisitor` trait (default `true`)
- [ ] When no visitor needs frontmatter, skip YAML accumulation and `serde_yaml_ng` parse (still read past `---` delimiters)

### In-memory link graph
- [ ] `LinkGraphVisitor` implementing `FileVisitor` (returns `false` from `needs_frontmatter`)
- [ ] Reuse existing `extract_links_from_text` — capture `(line_num, Link)` pairs per file
- [ ] `BacklinkEntry { source: PathBuf, line: usize, link: Link }` struct for reverse index entries
- [ ] Build `HashMap<PathBuf, Vec<BacklinkEntry>>` mapping target → list of sources
- [ ] Scan all `.md` files in vault directory
- [ ] E2e tests cover graph construction

### Backlinks
- [ ] `hyalo backlinks <file>` lists files that link to the given file
- [ ] Works with both `[[wikilink]]` and relative path links
- [ ] Shows line number and link text for each backlink
- [ ] `--format text` and `--format json` output modes
- [ ] E2e tests cover backlinks

### Move/rename command
- [ ] `hyalo move <old> <new>` renames file and updates all inbound wikilinks
- [ ] Uses in-memory graph to find inbound links, rewrites them in-place
- [ ] Handles both `[[path]]` and `[[path|alias]]` forms
- [ ] Dry-run mode (`--dry-run`) shows what would change without writing
- [ ] E2e tests cover move with link updates

### Shortest-path link resolution
- [ ] Obsidian-style `[[filename]]` resolves to shortest unambiguous path
- [ ] Ambiguous links reported as warnings
- [ ] E2e tests cover resolution

## Acceptance Criteria

- [ ] In-memory graph builds in under 1s for 1000-file vaults
- [ ] Backlinks query returns correct results
- [ ] Move command correctly updates all inbound links
- [ ] All quality gates pass (fmt, clippy, tests)

### Config-aware help text
- [ ] Move static `after_help` / example strings from derive attributes to runtime-generated strings
- [ ] Load `.hyalo.toml` before building the `clap::Command`
- [ ] Use `mut_arg()` to hide args that have config defaults (e.g. `--dir` when `dir` is set)
- [ ] Strip config-defaulted flags from all examples and cookbook snippets in help output
- [ ] Also strip from `--hints` output (verify existing `HintContext` logic covers this)
- [ ] E2e tests: help output without config shows `--dir`, help output with config omits it

## Notes

Scope is large — consider splitting into 39a (graph + backlinks) and 39b (move/rename + config-aware help) if needed.
