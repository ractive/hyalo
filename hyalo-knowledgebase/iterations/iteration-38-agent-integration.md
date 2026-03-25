---
title: "Iteration 38 — Agent Integration & Distribution"
type: iteration
date: 2026-03-23
tags: [iteration, llm, cli, distribution]
status: planned
branch: iter-38/agent-integration
---

# Iteration 38 — Agent Integration & Distribution

## Goal

Make hyalo a first-class tool for AI agents: a single `hyalo context` call orients a new conversation, the Claude Code skill is polished, and the skill is distributable as a plugin.

## Backlog items

- [[backlog/context-command]] (medium)
- [[backlog/done/claude-code-skill]] (medium)
- [[backlog/claude-plugin-distribution]] (medium)

## Tasks

### Context command
- [ ] `hyalo context` command exists
- [ ] Reads `.hyalo.toml` and reports effective defaults (dir, format, hints)
- [ ] Summarizes vault stats (file count, tags, properties, status values)
- [ ] Output is concise (under 20 lines) and designed for LLM context windows
- [ ] `--format json` returns structured data for programmatic use
- [ ] E2e tests cover context command

### Claude Code skill improvements
- [ ] Skill at `.claude/skills/hyalo-knowledgebase/SKILL.md` is comprehensive
- [ ] Reference files include complete command docs and query patterns
- [ ] CLAUDE.md hyalo section replaced with pointer to skill
- [ ] Verified skill auto-triggers on knowledgebase-related prompts

### Claude Code plugin distribution
- [ ] GitHub repository `ractive/hyalo-claude-plugin` exists
- [ ] `plugin.json` manifest with correct schema is present at repo root
- [ ] SKILL.md matches latest version from main repo
- [ ] `claude /plugin install --github-repo ractive/hyalo-claude-plugin` works
- [ ] Installed skill triggers correctly
- [ ] Main hyalo README documents the plugin install option

## Acceptance Criteria

- [ ] New conversation can orient itself with a single `hyalo context` call
- [ ] Skill is installable via Claude Code plugin system
- [ ] All quality gates pass (fmt, clippy, tests)
