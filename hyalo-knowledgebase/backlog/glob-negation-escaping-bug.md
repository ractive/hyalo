---
date: 2026-03-25
origin: dogfooding v0.3.1 against vscode-docs/docs
priority: medium
status: not-a-bug
tags:
- backlog
- bug
- filtering
- glob
title: Glob negation !pattern broken — exclamation mark gets backslash-escaped
type: backlog
---

# Glob negation !pattern broken

## Problem

`hyalo find --glob '!copilot/**/*.md'` fails with:

```
{"error": "no files match pattern", "path": "\\!copilot/**/*.md"}
```

The `!` prefix is being backslash-escaped before being passed to the glob matcher. The help text and cookbook both advertise glob negation as a feature, but it doesn't work.

## Repro

```bash
hyalo find --dir /path/to/vscode-docs/docs --glob '!copilot/**/*.md'
# => error: no files match pattern "\\!copilot/**/*.md"

hyalo find --dir /path/to/vscode-docs/docs --glob '!copilot/'
# => same error
```

## Resolution

**Not a bug.** The `!` character triggers zsh history expansion when not properly quoted, and Claude Code's Bash tool was escaping it as `\!`. With proper single quotes (`'!pattern'`), glob negation works correctly. Verified in iteration 40.
