---
date: 2026-03-23
origin: dogfooding help-text audit
priority: medium
status: wont-do
tags:
- backlog
- cli
- llm
- ux
title: Context command — generate AI agent configuration from vault state
type: backlog
---

## Problem

AI agents (Claude Code, etc.) repeatedly pass `--dir hyalo-knowledgebase/` even though `.hyalo.toml` already sets it as default. More broadly, agents lack awareness of the vault's current configuration and conventions unless manually told via CLAUDE.md or skill files.

## Proposal

A `hyalo context` command that generates a short, structured block of text designed for AI agent consumption. It would be dynamically generated from the actual `.hyalo.toml` and vault state — a single source of truth that's always accurate.

Example output:

```
# hyalo CLI — context for AI agents
Config: .hyalo.toml sets dir="hyalo-knowledgebase" — omit --dir from commands.
Format default: json (override with --format text for human-readable output).
Hints: disabled by default (use --hints for drill-down suggestions).
Vault: 54 files, 55 tags, 8 properties.
Status lifecycle values: planned, in-progress, completed, superseded, deferred, shelved.
Required frontmatter for type=iteration: title, type, date, tags, status, branch.
```

## Use Cases

- Paste into CLAUDE.md or a Claude Code skill as a dynamic include
- Pipe into agent context: `hyalo context >> .claude/skills/hyalo/reference/vault-state.md`
- CI step to regenerate agent context when vault structure changes

## Acceptance Criteria

- [x] `hyalo context` command exists
- [x] Reads `.hyalo.toml` and reports effective defaults (dir, format, hints)
- [x] Summarizes vault stats (file count, tags, properties, status values)
- [x] Output is concise (under 20 lines) and designed for LLM context windows
- [x] `--format json` returns structured data for programmatic use
