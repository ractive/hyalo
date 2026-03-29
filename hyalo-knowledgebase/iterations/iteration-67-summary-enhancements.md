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

- [x] Add dead-end detection: files with no outbound links (regardless of inbound)
- [x] Add `dead_ends` section to summary JSON output: `{"total": N, "files": [...]}`
- [x] Add dead-end line to summary text output
- [x] Add unit tests for dead-end detection
- [x] Add e2e tests for dead-end output in both JSON and text formats
- [x] Dogfood on external repos to verify dead-end counts are reasonable

See [[backlog/summary-dead-ends]]

### Show total count when --limit truncates

- [x] When `--limit` is active on `find`, compute total match count before truncating
- [x] JSON output: wrap in `{"total": N, "results": [...]}` envelope when `--limit` is used
- [x] Text output: append `showing N of M matches` line when `--limit` is used
- [x] When `--limit` is not used, output stays unchanged (no envelope)
- [x] Add e2e tests for `--limit` with total count in both formats
- [x] Verify `--limit` + `--jq` interaction works correctly (jq operates on the envelope)

See [[backlog/find-limit-total-count]]

### Quality gates

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
