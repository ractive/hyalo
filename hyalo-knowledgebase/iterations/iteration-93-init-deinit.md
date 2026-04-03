---
title: "Init improvements: deinit command + create missing directory"
type: iteration
date: 2026-04-01
tags:
  - init
  - cli
  - iteration
status: completed
branch: iter-93/init-deinit
---

## Goal

Round out the `init` / `deinit` lifecycle: add `hyalo deinit` to cleanly reverse `init`, and make `init --dir` create the target directory if it doesn't exist.

## Tasks

- [x] Add `hyalo deinit` command that removes all artifacts created by `init`
  - [x] Remove `.claude/skills/hyalo/SKILL.md` + empty parent dir
  - [x] Remove `.claude/skills/hyalo-tidy/SKILL.md` + empty parent dir
  - [x] Remove `.claude/rules/knowledgebase.md` + empty parent dir
  - [x] Remove empty `.claude/skills/` and `.claude/rules/` dirs
  - [x] Strip managed section from `.claude/CLAUDE.md` (preserve surrounding content)
  - [x] Remove `.hyalo.toml`
  - [x] Print summary of removed/skipped items
  - [x] Idempotent — safe to run when artifacts are already absent
- [x] `init --dir foo` creates `foo/` if it doesn't exist (only when `--dir` is explicit)
- [x] Add e2e tests for `deinit`
  - [x] Removes all artifacts created by `init --claude`
  - [x] Preserves non-managed content in `.claude/CLAUDE.md`
  - [x] Idempotent — running twice doesn't error
  - [x] Removes `.hyalo.toml`
- [x] Add e2e test for `init --dir` creating missing directory
- [x] Run quality gates: fmt, clippy, tests
- [x] Dogfood: run `hyalo init --claude && hyalo deinit` in a temp directory

## Related

- [[backlog/deinit-command]]
- [[backlog/init-create-missing-dir]]
