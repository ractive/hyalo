---
branch: iter-40/backlinks-robustness
date: 2026-03-25
status: in-progress
tags:
- iteration
- backlinks
- robustness
- bug-fix
title: Iteration 40 — Backlinks Robustness
type: iteration
---

# Iteration 40 — Backlinks Robustness

## Goal

Make the backlinks/link-graph code path as robust as the rest of the scanner. Currently, `LinkGraph::build` treats any scan error as fatal via `?`, while every other command (`find`, `summary`) uses `is_parse_error` to warn-and-skip malformed files.

## Backlog items

- [[backlog/backlinks-fatal-on-malformed-frontmatter]] (critical)
- ~~[[backlog/glob-negation-escaping-bug]]~~ — not a bug; glob negation works fine, the `!` was being shell-escaped by zsh history expansion

## Tasks

### Backlinks error recovery
- [x] Identify where the link graph scanner diverges from the normal scanner's error handling
- [x] Apply the same warn-and-skip strategy to the link graph builder
- [x] `backlinks` command completes on vaults with malformed files
- [x] `--fields backlinks` in `find` completes on vaults with malformed files
- [x] Warning messages include file path, exit code is 0
- [x] E2e test: backlinks on vault with a broken frontmatter file

### Quality gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [ ] Dogfood: `hyalo find --fields backlinks --dir ../docs/content` succeeds

## Acceptance Criteria

- [ ] Backlinks works on docs/content (3,517 files, 4 malformed) without fatal errors
- [x] All quality gates pass
