---
title: Iteration 38 — Agent Integration & Distribution
type: iteration
date: 2026-03-23
tags:
  - iteration
  - llm
  - cli
  - distribution
status: shelved
branch: iter-38/agent-integration
---

# Iteration 38 — Agent Integration & Distribution

## Goal

Make hyalo a first-class tool for AI agents: the Claude Code skill is polished and distributable as a plugin.

## Backlog items

- [[backlog/done/claude-code-skill]] (medium)
- [[backlog/done/claude-plugin-distribution]] (medium)

## Tasks

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

- [ ] Skill is installable via Claude Code plugin system
- [ ] All quality gates pass (fmt, clippy, tests)
