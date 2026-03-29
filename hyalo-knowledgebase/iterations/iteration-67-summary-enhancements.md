---
title: "Iteration 67: Summary command enhancements"
type: iteration
date: 2026-03-29
status: in-progress
branch: iter-67/summary-enhancements
tags:
  - iteration
  - summary
  - ux
---

# Iteration 67 — Summary Command Enhancements

## Goal

Improve the `summary` command and `find --limit` output based on dogfood findings from
the v0.6.0 tidy round (tested on 3 external repos: docs/content 3520 files, vscode-docs
339 files, legalize-es 8643 files).

## Tasks

### Dead-end detection in summary

- [x] Add dead-end detection: files that have inbound links and no outbound links (excluding orphans)
- [x] Add `dead_ends` section to summary JSON output: `{"total": N, "files": [...]}`
- [x] Add dead-end line to summary text output
- [x] Add unit tests for dead-end detection
- [x] Add e2e tests for dead-end output in both JSON and text formats
- [x] Dogfood on external repos to verify dead-end counts are reasonable

See [[backlog/summary-dead-ends]]

### Show total count when --limit truncates

- [x] Compute total match count; when `--limit` is active, compute before truncating
- [x] JSON output: always wrap in `{"total": N, "results": [...]}` envelope (stable schema)
- [x] Text output: append `showing N of M matches` line when `--limit` truncates
- [x] Add e2e tests for find JSON envelope (both limited and unlimited) and text --limit total count
- [x] Verify `--limit` + `--jq` interaction works correctly (jq operates on the envelope)

See [[backlog/find-limit-total-count]]

### Memory optimisation: find --limit with pre-sorted iteration

- [x] Scan path: pre-sort file list when `--sort file` + `--limit` + `!reverse`
- [x] Index path: pre-sort entries by any sort key (except backlinks_count) when `--limit` + `!reverse`
- [x] Skip FileObject construction once limit reached (count-only mode)
- [x] Both `find()` and `find_from_index()` paths
- [x] E2e test: deterministic results with `--sort file --limit N`
- [x] E2e test: accurate total with filters + `--sort file --limit`

See [[backlog/find-limit-memory-optimization]]

### Quality gates

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
