---
title: "Iteration 42 — UX Polish"
type: iteration
date: 2026-03-25
status: planned
branch: iter-42/ux-polish
tags: [iteration, ux, cli, polish]
---

# Iteration 42 — UX Polish

## Goal

Address UX friction found during dogfooding: config-aware help text, properties-typed naming inconsistency, --hints flag being a no-op on most commands, and backlinks not resolving site-absolute links. These improvements make hyalo more useful and less confusing.

## Backlog items

- [[backlog/config-aware-help-text]] (medium)
- [[backlog/backlinks-absolute-path-resolution]] (medium)
- [[backlog/properties-typed-naming-inconsistency]] (low)
- [[backlog/hints-flag-no-op-for-most-commands]] (low)

## Tasks

### Config-aware help text
- [ ] Load `.hyalo.toml` before building `clap::Command`
- [ ] Use `mut_arg()` to hide args that have config defaults (e.g. `--dir` when `dir` is set)
- [ ] Strip config-defaulted flags from examples and cookbook snippets in help output
- [ ] Strip from `--hints` output
- [ ] E2e tests: help output with/without config

### properties-typed naming
- [ ] Document the flag→key mapping discrepancy in `--help` long help
- [ ] Or: unify naming (pick one convention and apply consistently)

### Hints expansion
- [ ] Add meaningful `--hints` output to `find` (suggest narrowing filters, drill into specific files)
- [ ] Add `--hints` output to `properties summary` and `tags summary`
- [ ] Or: warn when `--hints` is used on a command that doesn't support it

### Absolute path link resolution
- [ ] Add `link-base` config option to `.hyalo.toml` schema
- [ ] Add `--link-base` CLI flag
- [ ] Strip prefix from absolute links before resolution in link graph
- [ ] Backlinks work on repos using site-absolute link conventions
- [ ] E2e test with absolute-path links and link-base

### Quality gates
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Dogfood: backlinks on vscode-docs with `--link-base /docs/` shows results

## Acceptance Criteria

- [ ] Help text hides `--dir` when `.hyalo.toml` sets it
- [ ] Naming inconsistency resolved or documented
- [ ] `--hints` either works everywhere or warns
- [ ] Backlinks functional on site-absolute link repos with `--link-base`
- [ ] All quality gates pass
