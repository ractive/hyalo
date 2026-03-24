---
branch: iter-23/help-text-improvements
date: 2026-03-23
status: completed
tags:
- iteration
- cli
- ux
- llm
title: 'Iteration 23b: Help text improvements'
type: iteration
---

## Goal

Audit and fix help text inconsistencies, missing documentation, and LLM-unfriendly gaps across all hyalo commands and subcommands. Driven by a systematic help system audit and dogfooding against the obsidian-hub vault (6520 files).

## Tasks

- [x] Audit all commands: both `-h` and `--help` for every command and subcommand
- [x] Test against external vault (obsidian-hub) for error handling quality
- [x] Fix `read --help` contradictory format default (said json and text simultaneously)
- [x] Add long-form help to task subcommands (read, toggle, set-status) with OUTPUT/SIDE EFFECTS/USE WHEN
- [x] Align `read --section` docs with `find --section` (level pinning, subsections, HEADING value name)
- [x] Add `find --section` to COMMAND REFERENCE synopsis
- [x] Add `find --section` examples to EXAMPLES and COOKBOOK
- [x] Clarify `task --line` is file-relative (including frontmatter)
- [x] Document `--fields` default behavior and always-present fields
- [x] Add `.hyalo.toml` sample to CONFIG section
- [x] Create backlog item for Claude Code skill

## Dogfooding Observations

- Warning behavior is inconsistent: `properties` and `tags` warn about broken frontmatter, but `summary` and `find` silently skip the same files
- Frontmatter parse warnings lack detail — "failed to parse YAML frontmatter" doesn't say what's wrong, making it hard for an LLM to fix the file
- Invalid property filter syntax (`--property "invalid>>filter"`) silently returns empty results instead of erroring
- Empty results in text mode produce no output at all (no "0 files matched" message)
- No fuzzy/prefix tag matching — `--tag dogfood` misses `dogfooding` with no suggestion
- Null tag entries (`tags:\n- \n`) and unquoted `@` aliases are valid in Obsidian but rejected by hyalo's parser
