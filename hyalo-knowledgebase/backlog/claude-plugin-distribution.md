---
title: Claude Code plugin distribution for hyalo skill
type: backlog
date: 2026-03-23
origin: skill packaging initiative
priority: medium
status: planned
tags:
  - backlog
  - cli
  - llm
  - ux
---

## Problem

Currently, to use the hyalo Claude Code skill, users must run `hyalo init --claude` in their project to generate `.claude/skills/hyalo/SKILL.md`. This requires having hyalo installed first and manually setting up each project. There is no way to distribute the skill as a standalone package that users can install directly into Claude Code.

## Proposal

Publish hyalo's Claude Code skill as a plugin package that can be installed via Claude Code's plugin system:

```bash
claude /plugin install --github-repo ractive/hyalo-claude-plugin
```

This requires:

1. **Create a dedicated GitHub repository** (`ractive/hyalo-claude-plugin`) to host the plugin
2. **Create a `plugin.json` manifest** at the repo root describing the plugin metadata, skill files, and dependencies
3. **Include the SKILL.md** and any reference files the skill needs
4. **Document installation** in both the plugin repo README and the main hyalo README

## Benefits

- Users can install the skill without having hyalo on PATH yet — the skill itself will prompt them to install the CLI
- Single command to add hyalo intelligence to any Claude Code project
- Plugin updates propagate automatically (depending on Claude Code's update mechanism)
- Lowers the barrier to entry: discover the plugin, install it, then install the CLI when prompted

## Acceptance Criteria

- [ ] GitHub repository `ractive/hyalo-claude-plugin` exists
- [ ] `plugin.json` manifest with correct schema is present at repo root
- [ ] SKILL.md is included and matches the latest version from the main repo
- [ ] `claude /plugin install --github-repo ractive/hyalo-claude-plugin` successfully installs the skill
- [ ] Installed skill triggers correctly on knowledgebase-related prompts
- [ ] Main hyalo README documents the plugin install option alongside `hyalo init --claude`
