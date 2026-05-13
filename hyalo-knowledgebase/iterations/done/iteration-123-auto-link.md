---
title: Iteration 123 — Auto-link unlinked mentions
type: iteration
date: 2026-04-17
status: completed
tags:
  - iteration
  - links
  - ux
  - feature
branch: iter-123/auto-link
---

## Goal

Add `hyalo links auto` — scan body text for unlinked mentions of known page titles and convert them to `[[wikilinks]]`. This turns a multi-hour manual linking task into a one-liner, leveraging hyalo's knowledge of the file graph.

Motivated by real dogfooding: a `/hyalo-tidy` session on a work vault required manually reading every file to find linkable mentions. See [[promotion-plan]] for the broader context of making hyalo useful for real-world KB maintenance.

## Design

### Title inventory

Build a lookup of linkable targets from three sources:
1. Filename stems (without `.md` and directory path)
2. Frontmatter `title` property
3. Frontmatter `aliases` property (list of alternate names)

Skip titles shorter than a configurable minimum (default: 3 characters) to avoid false positives on short common words like "CI", "API", "US".

### Matching engine

Use [Aho-Corasick](https://docs.rs/aho-corasick) multi-pattern matching for O(file_size) scanning regardless of pattern count. The `aho-corasick` crate is already a transitive dependency via `regex`.

Build the automaton **once** from the full title inventory, then reuse it for every file. Construction is O(total title characters) and takes single-digit milliseconds even for 10K+ titles. Do not rebuild per file.

- Case-insensitive matching by default
- Word-boundary constraints (don't match "Sprint" inside "Sprinting")
- Longest-match-first to prefer "Sprint Planning" over "Sprint"

### Exclusion zones

Skip matches inside:
- Frontmatter (already handled by scanner)
- Fenced code blocks and inline code
- Existing `[[wikilinks]]` and `[markdown](links)`
- Headings (auto-linking a heading looks wrong)
- The file's own title (no self-links)

The scanner infrastructure already handles most of these exclusion zones.

### Ambiguity handling

- If multiple files share the same title/stem, skip that title (no silent wrong linking)
- If a match overlaps with an existing link's text, skip it
- Prefer exact-case matches over case-insensitive ones

### Output

Follow existing patterns: JSON envelope by default, `--format text` for human-readable, `--dry-run` as default (safe).

```
$ hyalo links auto --format text
12 unlinked mentions found in 8 files:

  Meetings/2026-04-15.md:7    "Sprint Review" → [[Sprint Review]]
  Meetings/2026-04-15.md:12   "Bilas" → [[Bilas]]
  Common/onboarding.md:3      "Mail Templates" → [[Mail Templates]]
  ...

Pass --apply to write changes.
```

### CLI interface

```sh
hyalo links auto --dry-run          # preview (default)
hyalo links auto --apply            # write changes
hyalo links auto --file note.md     # single file
hyalo links auto --glob 'Common/*'  # scope to specific files
hyalo links auto --min-length 4     # skip short titles (default: 3)
hyalo links auto --exclude-title Common --exclude-title Mail  # manually exclude noisy titles
hyalo links auto --format text      # human-readable output
```

### How existing infrastructure helps

| Component | Role |
|-----------|------|
| `CaseInsensitiveIndex` + `stem_map` | Title inventory from filenames |
| `discovery::discover_files` | File enumeration |
| `scanner::scan_file_multi` | Body text with exclusion zones |
| `links::extract_links_from_text` | Detect already-linked regions |
| `link_graph` | Know what's already linked |
| Snapshot index | Provide title list without re-scanning (marginal benefit since body I/O dominates) |

### What does NOT help

BM25/FTS index: tokenizes and stems, so "running" matches "run". Wrong for auto-linking where exact surface-form matching is needed.

## Tasks

- [x] Add `aho-corasick` as a direct workspace dependency
- [x] Implement title inventory builder (stems + frontmatter title + aliases)
- [x] Implement body text scanner with exclusion zones (reuse scanner infrastructure)
- [x] Implement Aho-Corasick matching with word-boundary constraints
- [x] Implement ambiguity detection (skip titles shared by multiple files)
- [x] Implement dry-run output (JSON envelope + text format)
- [x] Implement `--apply` mode with atomic read-modify-write
- [x] Add `--min-length`, `--exclude-title`, `--glob` flags
- [x] Wire up as `hyalo links auto` subcommand
- [x] Add unit tests for matching edge cases (word boundaries, overlaps, self-links, ambiguity)
- [x] Add e2e tests
- [x] Update skill templates and README if needed

## Acceptance Criteria

- [x] `hyalo links auto --dry-run` finds unlinked mentions and shows proposed changes
- [x] `hyalo links auto --apply` writes correct `[[wikilinks]]` without breaking existing links
- [x] Self-links are excluded
- [x] Ambiguous titles (shared by multiple files) are skipped
- [x] Matches inside code blocks, existing links, and headings are skipped
- [x] Word boundaries are respected (no partial-word matches)
- [x] `--min-length` filters short titles
- [x] Works with `--index` for the title inventory phase
- [x] All quality gates pass
