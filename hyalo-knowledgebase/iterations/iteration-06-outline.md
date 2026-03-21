---
branch: iter-6/outline
date: 2026-03-21
status: in-progress
tags:
- iteration
- outline
- cli
- llm
title: Iteration 6 — Outline Command
type: iteration
---

# Iteration 6 — Outline Command

## Goal

Add an `outline` command that extracts the structural skeleton of markdown files — headings, frontmatter keys, wikilinks per section, and task counts per section — so an LLM can understand a document's structure without reading the full content.

## Motivation

An LLM working with a knowledge base needs to quickly decide *where* to look and *what* a document covers. Existing commands answer narrow questions: `properties` gives metadata, `tags` gives categorization, `links` gives outgoing references. But none answer "what's the structure of this document?"

The `outline` command fills this gap. It returns a section-aware table of contents enriched with just enough context (links, task progress) for an LLM to navigate confidently. This is the "what's in this file?" command.

## Relationship to Prior Work

- Builds on the streaming scanner from [[iteration-02-links]] (heading extraction reuses the code-block-aware line scanner)
- Reuses `collect_files()` from [[iteration-05-summary-list-refactor]] for `--file`/`--glob` targeting
- Complements `links` (which gives flat link lists) by adding section context to links
- Complements `properties` (which gives metadata) by showing body structure

## Design

### What the outline contains

**Per file:**
- `properties` — list of frontmatter properties with names, types, AND values (matching the `properties list` shape)
- `tags` — list of tag strings from frontmatter
- `sections` — ordered list of sections, each with:
  - `level` — heading level (1–6)
  - `heading` — heading text (stripped of leading `#` and whitespace)
  - `line` — line number in the file (1-based)
  - `links` — wikilinks and markdown links found between this heading and the next
  - `tasks` — `{ "total": N, "done": N }` if checkboxes exist in the section; omitted (not `null`) when no tasks
  - `code_blocks` — list of fenced code block languages in the section (e.g. `["rust", "json"]`), empty list if none

Content before the first heading is attributed to a synthetic section with `level: 0` and `heading: null` (only emitted if it contains links, tasks, or code blocks).

### Output format

JSON (default):
```json
{
  "file": "iterations/iteration-05-summary-list-refactor.md",
  "properties": [
    { "name": "title", "type": "text", "value": "Iteration 5 — Summary + List Subcommand Refactor" },
    { "name": "date", "type": "date", "value": "2026-03-21" },
    { "name": "tags", "type": "list", "value": ["iteration"] },
    { "name": "status", "type": "text", "value": "completed" },
    { "name": "branch", "type": "text", "value": "iter-5/summary-list-refactor" }
  ],
  "tags": ["iteration"],
  "sections": [
    {
      "level": 1,
      "heading": "Iteration 5 — Summary + List Subcommand Refactor",
      "line": 15,
      "links": [],
      "code_blocks": []
    },
    {
      "level": 2,
      "heading": "Relationship to iteration-04-property-find",
      "line": 27,
      "links": ["[[iteration-04-property-find]]"],
      "code_blocks": []
    },
    {
      "level": 2,
      "heading": "Tasks",
      "line": 31,
      "links": [],
      "tasks": { "total": 8, "done": 8 },
      "code_blocks": []
    }
  ]
}
```

Multi-file mode (with `--glob`) wraps results in an array.

Text format: indented tree with heading hierarchy, task counts inline.

### File targeting

Follows DEC-018:
- `--file` — outline of a single file
- `--glob` — outlines of all matching files
- Neither — outlines of all `.md` files under `--dir`

Unlike `links` (DEC-016, single-file only), outline supports multi-file mode because the output is lightweight (no full-body content) and useful for vault-wide structural overview.

## Tasks

### Core implementation
- [x] Add heading extraction to the streaming scanner (detect ATX headings `# ...` outside code blocks)
- [x] Implement section-aware accumulator that tracks current heading and collects links, tasks, code block languages per section
- [x] Extract frontmatter keys with types (reuse existing type inference from `frontmatter.rs`)
- [x] Build `outline` command with `--file`/`--glob` support using `collect_files()`
- [x] Handle pre-heading content (level 0 synthetic section, only if non-empty)

### Typed structs refactor
- [x] Introduce `src/types.rs` with `#[derive(Serialize)]` structs for all JSON output shapes (DEC-025)
- [x] Refactor `properties`, `tags`, `links` commands to use typed structs instead of `json!()` macros
- [x] Remove `build_find_json` / `build_list_mutation_json` generic helpers

### Output formats
- [x] JSON output with the schema described above
- [ ] Text output as indented tree (2-space indent per heading level, task counts inline)

### Code quality
- [x] Add unit tests for heading extraction (ATX headings, inside/outside code blocks, edge cases)
- [x] Add unit tests for section accumulation (links, tasks, code blocks attributed to correct section)
- [x] Add e2e tests for `outline --file`, `outline --glob`, and vault-wide mode
- [x] Run quality gates: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`

### Documentation
- [x] Add decision log entry DEC-024 for outline command design
- [x] Add decision log entry DEC-025 for typed structs
- [x] Update iteration plan to reflect iteration 6 scope change
- [x] Update README.md with `outline` command usage

## Design Notes

- ATX headings only (`# heading`). Setext headings (underlined with `===`/`---`) are not supported — they're rare in Obsidian vaults and complicate streaming parsing.
- Heading text is returned as-is after stripping `#` prefix and whitespace. Inline formatting (`**bold**`, `*italic*`) is preserved — the LLM can handle it.
- Links in sections include both `[[wikilinks]]` and `[markdown](links)` — reuse the scanner's existing link extraction.
- Code block language is the info string after the opening fence (e.g. ` ```rust ` → `"rust"`). Empty info strings produce no entry.
- The streaming scanner processes files line-by-line, so this command never buffers an entire file body. Frontmatter keys are read separately via `read_frontmatter()`.
