---
title: Global --count flag for list commands
type: iteration
date: 2026-03-31
tags:
  - iteration
  - cli
  - ux
status: completed
branch: iter-88/count-flag
---

## Goal

Add a `--count` global flag that prints only the total count as a bare integer, replacing the common `--jq '.total'` pattern with a shorter, more ergonomic alternative.

## Tasks

- [x] Add `--count` bool field to `Cli` global args
- [x] Add `count` field to `OutputPipeline` and handle in `finalize()`
- [x] Add `--count` + `--jq` conflict validation in `run.rs`
- [x] Handle `--count` in `RawOutput` arm (non-list command error)
- [x] Update CLI help text (`long_about`)
- [x] Write E2E tests (`e2e_count.rs`)
- [x] Update README.md
- [x] Update SKILL.md
- [x] Create iteration file
- [x] Create PR and review
