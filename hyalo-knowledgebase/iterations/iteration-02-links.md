---
title: "Iteration 2 — Wikilink Parser & Link Commands"
type: iteration
date: 2026-03-20
status: in-progress
branch: iter-2/wikilink-parser-link-commands
tags:
  - iteration
  - links
  - scanner
---

# Iteration 2 — Wikilink Parser & Link Commands

## Goal

Parse `[[wikilinks]]`, `![[embeds]]`, and `[markdown](links)` from markdown files. Extract and resolve internal links. Provide CLI commands to list outgoing links and find broken links.

## CLI Interface

```sh
# Outgoing links from a file (or all files)
hyalo links [--path <file.md>] [--format json|text]

# Links that don't resolve to any file
hyalo unresolved [--path <file.md>] [--format json|text]
```

## New Modules

### `src/scanner.rs` — Streaming Markdown Scanner

Reusable line-by-line streaming scanner. Skips frontmatter, fenced code blocks, and inline code spans. Calls visitor function for each text segment with line number. Supports early abort via `ScanAction::Stop`.

### `src/links.rs` — Link Extraction

Uses scanner to extract links from text segments. Handles wikilinks (`[[Note]]`, `[[Note|Display]]`, `[[Note#Heading]]`, `[[Note#^block-id]]`), embeds (`![[Note]]`), and markdown links (`[text](note.md)`). Skips external links (http/https/mailto).

### `src/graph.rs` — Link Resolution

File index maps lowercased stems to relative paths. Resolves link targets using Obsidian shortest-path resolution. Path-qualified names use exact match.

### `src/commands/links.rs` — Command Implementations

Two commands: `links` (outgoing links) and `unresolved` (broken links). Both support single file or vault-wide scanning.

## Tasks

### Scanner
- [x] Implement streaming line scanner with frontmatter skipping
- [x] Track fenced code block state (backtick and tilde fences)
- [x] Strip inline code spans
- [x] Support early abort via `ScanAction::Stop`
- [x] Extract reusable `skip_frontmatter` helper in frontmatter.rs

### Link Extraction
- [x] Parse wikilinks: `[[Note]]`, `[[Note|Display]]`, `[[Note#Heading]]`, `[[Note#^block-id]]`
- [x] Parse embeds: `![[Note]]`, `![[image.png]]`
- [x] Parse markdown links: `[text](note.md)`, `[text](sub/dir/note.md)`
- [x] Skip external links (http/https/mailto)
- [x] Track line numbers for all links

### Link Resolution
- [x] Build file index from vault directory
- [x] Resolve simple stems (Obsidian shortest-path)
- [x] Resolve path-qualified targets
- [x] Case-insensitive resolution

### Commands
- [x] `links` command — single file and vault-wide
- [x] `unresolved` command — single file and vault-wide
- [x] Wire up CLI in main.rs

### Testing
- [x] Unit tests for scanner (14 tests)
- [x] Unit tests for link extraction (14 tests)
- [x] Unit tests for graph resolution (8 tests)
- [x] Unit tests for commands (7 tests)
- [x] E2E tests for `links` command (9 tests)
- [x] E2E tests for `unresolved` command (4 tests)

### Quality Gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`

### Dogfooding
- [x] `hyalo links --dir hyalo-knowledgebase` — finds wikilinks in iteration/research files
- [x] `hyalo unresolved --dir hyalo-knowledgebase` — all links resolve (empty result)

## Known Limitations

- **`%%comments%%` not handled:** Obsidian comment blocks (`%%...%%`) are not yet tracked by the scanner. Links inside comments will be incorrectly extracted. Can be added to the scanner later since we control the code.
- **No nested bracket handling:** Edge cases like `[[link with [brackets]]]` are not supported.

## Deferred to Indexing Iteration

The following commands require a full vault scan per invocation and are deferred to the indexing iteration (SQLite-backed):

- `backlinks` — which files link to a given file
- `orphans` — files with no incoming links
- `deadends` — files with no outgoing links

## Acceptance Criteria

1. [x] `hyalo links --path file.md` outputs all outgoing links as JSON with target, style, line, display, heading, block_ref, is_embed
2. [x] `hyalo links` scans all files and outputs links per file
3. [x] `hyalo unresolved --path file.md` outputs only links that don't resolve to any file in the vault
4. [x] `hyalo unresolved` scans all files for unresolved links
5. [x] Links inside fenced code blocks and inline code spans are skipped
6. [x] External links (http/https/mailto) are excluded
7. [x] All tests pass: `cargo fmt && cargo clippy && cargo test`
