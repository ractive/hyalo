---
title: "Replace ~= with /regex/ delimiters for --title and --section"
type: iteration
date: 2026-03-31
tags:
  - iteration
  - cli
  - ux
  - breaking-change
status: in-progress
branch: iter-89/slash-regex-syntax
---

## Goal

Replace the verbose `~=/regex/` prefix syntax with direct `/regex/` delimiters for `--title` and `--section` flags. The `/regex/` pattern (awk, sed, JavaScript) is universally recognized and saves keystrokes. `--property K~=pattern` is unchanged (it's an operator, not a prefix).

## Before → After

```bash
--title '~=/^The/i'         →  --title '/^The/i'
--section '~=/DEC-03[12]/'  →  --section '/DEC-03[12]/'
```

## Tasks

- [x] Replace `~=` detection with `/regex/` in `TitleMatcher::parse()` (build.rs)
- [x] Remove `looks_like_misused_regex()` heuristic and warning
- [x] Replace `~=` detection with `/regex/` in `SectionFilter::parse()` (heading.rs)
- [x] Remove `parse_section_regex()` helper
- [x] Update CLI help text (args.rs)
- [x] Update e2e tests (e2e_find.rs)
- [x] Update unit tests (heading.rs)
- [x] Update README.md and SKILL.md
- [x] Create iteration file
- [ ] Create PR and review
