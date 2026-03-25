---
branch: iter-42/ux-polish
date: 2026-03-25
status: completed
tags:
- iteration
- ux
- cli
- polish
title: Iteration 42 — UX Polish
type: iteration
---

# Iteration 42 — UX Polish

## Goal

Address UX friction found during dogfooding: config-aware help text, properties-typed naming inconsistency, --hints flag being a no-op on most commands, and backlinks not resolving site-absolute links. These improvements make hyalo more useful and less confusing.

## Backlog items

- [[backlog/config-aware-help-text]] (medium)
- [[backlog/done/backlinks-absolute-path-resolution]] (medium)
- [[backlog/properties-typed-naming-inconsistency]] (low)
- [[backlog/hints-flag-no-op-for-most-commands]] (low)

## Tasks

### Config-aware help text
- [x] Load `.hyalo.toml` before building `clap::Command`
- [x] Use `mut_arg()` to hide args that have config defaults (e.g. `--dir` when `dir` is set)
- [ ] Strip config-defaulted flags from examples and cookbook snippets in help output
- [ ] Strip from `--hints` output
- [x] E2e tests: help output with/without config

### properties-typed naming
- [x] Document the flag→key mapping discrepancy in `--help` long help

### Hints expansion
- [x] Add meaningful `--hints` output to `find` (suggest narrowing filters, drill into specific files)
- [x] Warn when `--hints` is used on a command that doesn't support it (mutation commands)

### Absolute path link resolution
- [x] Strip `/<dir>/` prefix from absolute links before resolution in link graph (no new config — derive from existing `dir`)
- [x] Backlinks work on repos using site-absolute link conventions
- [x] E2e test with absolute-path links

### Quality gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [ ] Dogfood: backlinks on vscode-docs (with `dir = "docs"`) resolves absolute links

## Acceptance Criteria

- [x] Help text hides `--dir` when `.hyalo.toml` sets it
- [x] Naming inconsistency resolved or documented
- [x] `--hints` either works everywhere or warns
- [x] Backlinks functional on site-absolute link repos (prefix derived from `dir`)
- [x] All quality gates pass
