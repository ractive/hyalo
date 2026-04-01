---
title: "Add hyalo deinit command"
type: backlog
date: 2026-04-01
tags:
  - init
  - ux
  - cli
status: planned
priority: low
---

## Problem

There is no way to reverse `hyalo init`. If a user wants to remove hyalo's configuration from a project, they have to manually delete files.

## Expected behaviour

`hyalo deinit` removes everything that `init` and `init --claude` created:

- `.claude/skills/hyalo/SKILL.md` and empty parent dir
- `.claude/skills/hyalo-tidy/SKILL.md` and empty parent dir
- `.claude/rules/knowledgebase.md` and empty parent dir
- Managed section (`<!-- hyalo:start -->` … `<!-- hyalo:end -->`) from `.claude/CLAUDE.md`
- `.hyalo.toml`

Clean up empty directories left behind (`.claude/skills/`, `.claude/rules/`), and remove `.claude/` itself only if it has become empty so that any other user content is preserved.

## Implementation notes

- New subcommand `Deinit` in `args.rs`
- New `commands/deinit.rs` module
- Reuse `SECTION_START`/`SECTION_END` constants from `init.rs` for managed section removal
- Print summary of what was removed (and what was already absent)
- Idempotent — safe to run multiple times
