---
branch: iter-49/init-improvements
date: 2026-03-26
status: completed
tags:
- cli
- init
- claude-code
- iteration
title: Improve `hyalo init` — overwrite, rules, smart dir detection
type: iteration
---

## Goal

Make `hyalo init --claude` a reliable one-command setup that installs the latest skills,
rule, and CLAUDE.md hint — parameterized with the detected (or user-specified) knowledgebase
directory. Re-running it should update everything to the latest version.

## Tasks

- [x] Smart dir detection: pick the directory with the most `.md` files (recursive count), respect `--dir` flag override
- [x] Change skill/rule installation from skip to overwrite — always write the latest embedded content
- [x] Install `.claude/rules/knowledgebase.md` as part of `--claude`, with `paths:` using the detected dir
- [x] Parameterize rule paths with the detected dir
- [x] Hide irrelevant global flags (`--jq`, `--format`, `--index`, `--hints`) from `hyalo init --help` — blocked: clap `global = true` args can't be hidden per-subcommand via `mut_subcommand`
- [x] Keep `.hyalo.toml` skip behavior (don't overwrite user config) but update dir if re-running with explicit `--dir`
- [x] Update e2e tests for new overwrite behavior, rule installation, smart dir detection
- [x] Refactor `run_init` to accept `cwd` parameter for testability (no more `set_current_dir` races)
