---
title: "Section-scoped filter for find command"
type: iteration
date: 2026-03-23
tags:
  - iteration
  - search
  - cli
status: in-progress
branch: iter-22/section-filter
---

# Iteration 22: Section Filter for Find

## Goals

Add `--section HEADING` filter to the `find` command, scoping body-level results (tasks, content matches, sections) to matching headings.

## Tasks

- [x] Create shared `src/heading.rs` module (consolidated heading parser + `SectionFilter` + `build_section_scope`)
- [x] Replace 3 duplicate heading parsers with shared module
- [x] Refactor `read --section` to use shared `SectionFilter`
- [x] Add `--section` CLI arg to `find` command
- [x] Implement section scope filtering in `find()` (line-range-based, handles nesting)
- [x] Add unit tests for heading parser, SectionFilter, and scope builder
- [x] Add e2e tests for section filter (10 tests)
- [x] Update help texts and README
- [x] Run code quality gates (fmt, clippy, test)
- [ ] Create PR

## Matching Semantics

- Without `#` prefix: case-insensitive whole-string match at any heading level
- With `#` prefix: case-insensitive whole-string match pinned to that level
- Multiple `--section` values: OR semantics
- Children included: content under nested subsections is in scope
- Output `section` field: nearest heading (unchanged behavior)
