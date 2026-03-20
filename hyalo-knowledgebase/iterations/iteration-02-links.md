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

## Acceptance Criteria (original — superseded by addendum)

1. [x] `hyalo links --path file.md` outputs all outgoing links as JSON with target, style, line, display, heading, block_ref, is_embed
2. [x] `hyalo links` scans all files and outputs links per file
3. [x] `hyalo unresolved --path file.md` outputs only links that don't resolve to any file in the vault
4. [x] `hyalo unresolved` scans all files for unresolved links
5. [x] Links inside fenced code blocks and inline code spans are skipped
6. [x] External links (http/https/mailto) are excluded
7. [x] All tests pass: `cargo fmt && cargo clippy && cargo test`

---

## Addendum: Scope Revision (2026-03-20)

After reviewing the initial implementation, we decided the commands are over-engineered for the actual consumer (AI agents). This addendum redefines what `links` and `unresolved` should do and plans cleanup of unused code.

### Design Decisions

**Why vault-wide `links` is wrong:** An AI agent works on one file at a time. Dumping outgoing links for all files produces bulk data that's expensive to generate and hard to act on. If the agent needs links for multiple files, it can call the command per file. Bulk graph operations belong in a future `graph` command backed by an index.

**Why vault-wide `unresolved` is wrong:** Obsidian's vault-wide broken-link view is a human dashboard feature. For an AI agent, "which links in *this* file are broken?" is the actionable question. Vault-wide unresolved scanning requires building a full file index on every invocation — O(N) directory walk + O(M) file reads — with no caching. Not worth it without an index.

**Why most link metadata fields are unnecessary:** Fields like `style` (wiki vs markdown), `line`, `is_embed`, `heading`, `block_ref` are parser internals. An AI agent needs to know *where a link points* and *what it's called*, not how the link was syntactically written. Start minimal, add fields later if a concrete use case emerges.

**Why `--file` instead of `--path`:** The `--path` flag on `properties` supports globs for multi-file queries, which makes sense there (e.g. "show all tags across research notes"). For `links` and `unresolved`, multi-file output adds complexity without value. Using `--file` signals "exactly one file" and avoids confusion with the glob-capable `--path`.

**Why `target` should be a resolved path:** AI agents work with file paths, not Obsidian note names. `[[My Note]]` is meaningless to an agent — it needs `notes/my-note.md` to open the file. The link object should include both the raw target (for display/editing) and the resolved path (for navigation). Unresolved links have `resolved_path: null`.

### Revised CLI Interface

```sh
# Outgoing links from exactly one file (--file is required)
hyalo links --file <file.md> [--format json|text]

# Broken links in exactly one file (--file is required)
hyalo unresolved --file <file.md> [--format json|text]
```

### Revised Link Object

```json
{
  "target": "My Note",
  "path": "notes/my-note.md",
  "label": "display text"
}
```

- **`target`** — raw target as written in the source file (for search/replace, display)
- **`path`** — resolved file path relative to `--dir`, or `null` if broken
- **`label`** — display text from `[[target|label]]` or `[label](target.md)`, or `null` if none

All other fields (`style`, `line`, `is_embed`, `heading`, `block_ref`) are dropped from the output. The parser may still extract them internally but they don't surface in the API.

### Revised Acceptance Criteria

1. [x] `hyalo links --file note.md` outputs outgoing links with `target`, `path`, `label`
2. [x] `hyalo unresolved --file note.md` outputs only links where `path` is null
3. [x] `--file` is required for both commands (no vault-wide mode)
4. [x] Links inside fenced code blocks and inline code spans are skipped
5. [x] External links (http/https/mailto) are excluded
6. [x] All quality gates pass: `cargo fmt && cargo clippy && cargo test`

### Implementation Tasks

#### Simplify Link struct
- [x] Remove `style`, `line`, `is_embed`, `heading`, `block_ref` fields from `Link` (or stop surfacing them)
- [x] Add `path: Option<String>` to the output
- [x] Rename `display` to `label`

#### Revise commands
- [x] Change `links` command: require `--file`, remove vault-wide mode, resolve each link via `FileIndex`
- [x] Change `unresolved` command: require `--file`, remove vault-wide mode, filter to `resolved_path == null`
- [x] Update CLI in `main.rs`: replace `--path` with `--file` for both commands, make it required

#### Remove `unsafe` in scanner
- [x] Replace `unsafe { result.as_bytes_mut() }` in `strip_inline_code` with safe `replace_range` or `Cow`-based approach

#### Performance: avoid redundant allocations
- [x] Change `strip_inline_code` to return `Cow<'_, str>` — borrow when no backticks present

#### Cleanup unused code
- [x] Remove `graph.rs` `FileIndex::build` → replace with a method that accepts an already-discovered file list to avoid double directory walk
- [x] Remove vault-wide functions: `links_all`, `unresolved_all` from `commands/links.rs`
- [x] Remove vault-wide tests and update existing tests
- [x] Clean up `link_to_json` — simplify to match new minimal link object

#### Update tests
- [x] Update unit tests in `links.rs` for simplified `Link` struct
- [x] Update unit tests in `commands/links.rs` for single-file-only behavior
- [x] Update e2e tests: remove vault-wide tests, update JSON assertions for new shape
- [x] Add e2e test: `links` without `--file` fails with helpful error
- [x] Add e2e test: verify `path` is populated for valid links and null for broken ones

#### Dogfooding
- [x] `hyalo links --file iterations/iteration-02-links.md --dir hyalo-knowledgebase`
