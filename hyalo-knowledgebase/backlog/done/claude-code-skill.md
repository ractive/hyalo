---
date: 2026-03-23
origin: dogfooding help-text audit
priority: medium
status: completed
tags:
- backlog
- cli
- llm
- ux
title: Claude Code skill for hyalo knowledgebase interactions
type: backlog
---

## Problem

The CLAUDE.md file contains hyalo usage instructions interleaved with Rust conventions, PR discipline, and other concerns. This is always loaded into context even when the knowledgebase isn't relevant. A dedicated Claude Code skill with `user-invocable: false` would:

- Only load its full body when knowledgebase work is detected (progressive disclosure)
- Keep reference material (command reference, output shapes, query patterns) in `reference/` files
- Be self-contained and easier to maintain than CLAUDE.md sections

## Proposed Structure

```
.claude/skills/hyalo-knowledgebase/
├── SKILL.md              # Core rules + command cheat sheet
└── reference/
    ├── commands.md        # Full command reference with examples + output shapes
    └── query-patterns.md  # Common workflows (find by status, bulk set, etc.)
```

## SKILL.md Content

- Core rule: always use hyalo CLI, never Edit/Read/Grep on knowledgebase files
- Command cheat sheet: which command for which task
- `.hyalo.toml` awareness: document that `--dir` is already configured — agents must NOT pass it explicitly (this is a recurring problem; see also [[context-command]])
- Output format guidance (--format text vs JSON vs --jq)
- `--hints` flag recommendation for discovery
- Frontmatter conventions (required fields per file type)
- Status lifecycle (planned → in-progress → completed → superseded)
- Error recovery patterns (section not found, file not found)

## Acceptance Criteria

- [ ] Skill created at `.claude/skills/hyalo-knowledgebase/SKILL.md`
- [ ] Reference files created with complete command docs and query patterns
- [ ] CLAUDE.md hyalo section replaced with pointer to skill
- [ ] Verified skill auto-triggers on knowledgebase-related prompts
