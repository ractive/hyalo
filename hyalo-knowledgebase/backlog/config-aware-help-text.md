---
title: Config-aware help text
type: backlog
date: 2026-03-25
origin: iteration-39 scope split
priority: medium
status: planned
tags: [backlog, cli, ux, help]
---

# Config-aware help text

## Problem

When `.hyalo.toml` sets a default `dir`, the `--dir` flag still appears in help text, examples, and `--hints` output. This is confusing — users see flags they don't need.

## Proposal

Load `.hyalo.toml` before building the `clap::Command` and dynamically hide args that have config defaults.

## Tasks

- [ ] Move static `after_help` / example strings from derive attributes to runtime-generated strings
- [ ] Load `.hyalo.toml` before building the `clap::Command`
- [ ] Use `mut_arg()` to hide args that have config defaults (e.g. `--dir` when `dir` is set)
- [ ] Strip config-defaulted flags from all examples and cookbook snippets in help output
- [ ] Also strip from `--hints` output (verify existing `HintContext` logic covers this)
- [ ] E2e tests: help output without config shows `--dir`, help output with config omits it

## References

Originally part of [[iterations/iteration-39a-link-graph]], split out during scope reduction.
