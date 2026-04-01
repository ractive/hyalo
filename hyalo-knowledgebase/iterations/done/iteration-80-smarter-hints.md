---
title: Context-aware smarter hints
type: iteration
date: 2026-03-30
tags:
  - ux
  - feature
  - dogfooding
status: completed
priority: 2
branch: iter-80/smarter-hints
---

## Goal

Make hints context-aware so they suggest relevant next queries based on the current results, turning hyalo into a self-teaching tool for both humans and LLMs.

## Context

After [[iterations/done/iteration-79-slim-defaults]] makes hints default, the next step is making them actually smart. Current hints are static/generic. Context-aware hints should look at the results and suggest the most useful drill-down based on what's there.

This is an ongoing effort — hint quality should be iterated on over time.

## Ideas

- Suggest `--fields tasks` when results contain files with tasks
- Suggest `--fields backlinks` for single-file queries
- Suggest `--fields all` when default fields are in use
- Suggest `--jq '.total'` when result count is large
- Suggest `--sort date --reverse` when results span a date range
- Suggest `-e 'regex'` when a literal body search returns many results (could be narrowed with regex)
- Suggest `--limit N` when results are truncated or very large
- Suppress hints that would repeat the current query's options

## Tasks

- [x] Audit current hint generation — identify what's static vs context-aware
- [x] Implement context-aware hint logic based on result shape
- [x] Add tests for conditional hint generation
- [x] Iterate on hint quality based on dogfooding feedback
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
