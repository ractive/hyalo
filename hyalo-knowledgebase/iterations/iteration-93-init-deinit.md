---
title: "Init improvements: deinit command + create missing directory"
type: iteration
date: 2026-04-01
tags:
  - init
  - cli
  - iteration
status: planned
branch: iter-93/init-deinit
---

## Goal

Round out the `init` / `deinit` lifecycle: add `hyalo deinit` to cleanly reverse `init`, and make `init --dir` create the target directory if it doesn't exist.

## Tasks

- [ ] Add `hyalo deinit` command that removes all artifacts created by `init`
  - [ ] Remove `.claude/skills/hyalo/SKILL.md` + empty parent dir
  - [ ] Remove `.claude/skills/hyalo-tidy/SKILL.md` + empty parent dir
  - [ ] Remove `.claude/rules/knowledgebase.md` + empty parent dir
  - [ ] Remove empty `.claude/skills/` and `.claude/rules/` dirs
  - [ ] Strip managed section from `.claude/CLAUDE.md` (preserve surrounding content)
  - [ ] Remove `.hyalo.toml`
  - [ ] Print summary of removed/skipped items
  - [ ] Idempotent — safe to run when artifacts are already absent
- [ ] `init --dir foo` creates `foo/` if it doesn't exist (only when `--dir` is explicit)
- [ ] Add e2e tests for `deinit`
  - [ ] Removes all artifacts created by `init --claude`
  - [ ] Preserves non-managed content in `.claude/CLAUDE.md`
  - [ ] Idempotent — running twice doesn't error
  - [ ] Removes `.hyalo.toml`
- [ ] Add e2e test for `init --dir` creating missing directory
- [ ] Run quality gates: fmt, clippy, tests
- [ ] Dogfood: run `hyalo init --claude && hyalo deinit` in a temp directory

## Related

- [[backlog/deinit-command]]
- [[backlog/init-create-missing-dir]]
