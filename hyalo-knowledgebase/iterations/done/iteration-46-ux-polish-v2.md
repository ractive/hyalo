---
branch: iter-46/ux-polish-v2
date: 2026-03-26
status: completed
tags:
- iteration
- ux
- dogfooding
title: Iteration 46 — UX Polish v2 (Dogfood v0.4.1 Follow-ups)
type: iteration
---

# Iteration 46 — UX Polish v2

## Goal

Address all UX issues and minor bugs found during v0.4.1 dogfooding on GitHub Docs (3520 files) and VS Code Docs (339 files). All items are small, self-contained fixes.

## Backlog items

- [[backlog/done/repeatable-glob-flag]]
- [[backlog/done/sort-by-backlinks-count]]
- [[backlog/done/trailing-slash-link-resolution]]
- [[backlog/done/query-string-link-resolution]]
- [[backlog/done/limit-zero-means-unlimited]]
- [[backlog/done/bare-subcommand-defaults]]
- [[backlog/done/empty-body-pattern-matches-all]]

## Tasks

### Repeatable --glob (medium)
- [ ] Change `--glob` clap arg from `Option<String>` to `Vec<String>` across all commands
- [ ] Update `match_glob` in `discovery.rs` to accept multiple patterns via `GlobSet`
- [ ] Separate positive and negative patterns; apply positive first, then filter out negatives
- [ ] Update `collect_files` in `commands/mod.rs` to handle `Vec<String>`
- [ ] E2e tests: multiple globs, mixed positive+negative, single glob backward compat
- [ ] Update help text and README

### Sort by backlinks_count / links_count (medium)
- [ ] Add `BacklinksCount` and `LinksCount` variants to `SortField` enum in `filter.rs`
- [ ] Accept `backlinks_count` and `links_count` in `parse_sort`
- [ ] In find command: force backlinks computation when sort is `BacklinksCount`
- [ ] Disable `--limit` short-circuit when sort is `BacklinksCount` or `LinksCount`
- [ ] E2e tests: sort by both new fields, verify ordering

### Link resolution: trailing slash and query strings (low)
- [ ] In `resolve_target` (`discovery.rs`): strip trailing `/` from target
- [ ] In `resolve_target`: strip `?...` query string and `#...` fragment before lookup
- [ ] Unit tests for both edge cases

### --limit 0 = unlimited (low)
- [ ] In find command: convert `Some(0)` to `None` before passing limit
- [ ] E2e test: `--limit 0` returns all files

### Bare tags/properties defaults to summary (low)
- [ ] Make `summary` the default subcommand for `tags` and `properties`
- [ ] E2e tests: bare `hyalo tags` and `hyalo properties` produce summary output

### Empty body pattern warning (low)
- [ ] Emit stderr warning when body pattern is empty string
- [ ] E2e test: `find ""` warns on stderr, still returns results

### Quality gates
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`

## Acceptance Criteria

- [ ] `--glob` is repeatable: `--glob 'a/**' --glob '!a/**/index.md'` works
- [ ] `--sort backlinks_count` and `--sort links_count` produce correctly ordered results
- [ ] Trailing-slash and query-string links resolve correctly
- [ ] `--limit 0` behaves as unlimited
- [ ] Bare `hyalo tags` and `hyalo properties` default to summary
- [ ] Empty body pattern emits a warning
- [ ] All quality gates pass
