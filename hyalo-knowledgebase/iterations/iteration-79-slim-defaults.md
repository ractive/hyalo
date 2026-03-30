---
title: "Slim default fields and make --hints the default"
type: iteration
date: 2026-03-30
tags:
  - ux
  - dogfood
  - breaking-change
status: planned
priority: 2
branch: iter-79/slim-defaults
---

## Goal

Make output more useful by default: enable `--hints` by default (opt out with `--no-hints`) and remove `tasks` from the default `--fields`.

## Context

Found during v0.6.0 dogfooding (iteration 74). Two problems:

1. **Hints are never used** — neither humans nor LLMs pass `--hints`, so the drill-down suggestions go unseen. Making hints default means every query teaches the user (or LLM) what deeper queries are available. `--no-hints` and `--jq` already suppress them.

2. **Default output is too verbose** — `sections`, `tasks`, and `links` are included by default, making `--format text` output a wall of text for simple filter queries. Tasks in particular are rarely needed in list output and add significant noise.

See also: [[iteration-80-smarter-hints]] for making hints context-aware.

## Tasks

- [ ] Make `--hints` the default (flip the default in the hints logic)
- [ ] Keep `--no-hints` as opt-out
- [ ] Keep `--jq` suppressing hints (already the case)
- [ ] Remove `tasks` from default `--fields` (require `--fields tasks` or `--fields all` to include)
- [ ] Update `Fields::default()` in `hyalo-core/src/filter/fields.rs`
- [ ] Update `.hyalo.toml` schema/docs if hints default is stored there
- [ ] Update help text in `args.rs` to reflect new defaults
- [ ] Update `SKILL.md` to instruct LLMs to read and follow hints
- [ ] Update `CLAUDE.md` to mention hints as a navigation aid
- [ ] Update e2e tests that depend on default field/hint output
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
