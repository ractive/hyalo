---
title: "Discoverable drill-down commands in summary output (HATEOAS for CLI)"
type: backlog
date: 2026-03-21
status: completed
priority: medium
origin: dogfooding iteration-08
tags:
  - backlog
  - cli
  - llm
  - ux
---

# Discoverable drill-down commands in summary output

## Problem

When `properties summary` or `tags summary` shows aggregate counts, there's no indication of _how_ to drill down. The user (or an LLM agent) has to already know the full command tree to go from "status (text): 12 files" to `hyalo property find --name status`.

## Proposal

In `--format text` output, append runnable commands as hints after each summary line. In `--format json` output, include a `commands` or `links` field with the drill-down commands.

### Text output example

```
6 unique properties
  status (text): 12 files
    → hyalo property find --name status
  title (text): 15 files
    → hyalo property find --name title
  tags (list): 10 files
    → hyalo tags list

8 unique tags
  rust: 7 files
    → hyalo tag find --name rust
  backlog: 12 files
    → hyalo tag find --name backlog
```

### JSON output example

```json
[
  {
    "name": "status",
    "type": "text",
    "count": 12,
    "links": {
      "find": "hyalo property find --name status",
      "list": "hyalo properties list"
    }
  }
]
```

## Precedents in other CLIs

- `kubectl` prints "Use `kubectl describe pod/foo` for details" after resource listings
- `gh` (GitHub CLI) suggests follow-up commands like "To see the full diff, run: gh pr diff 123"
- `docker` suggests next steps, e.g. "Run `docker scan` to find vulnerabilities"
- `npm` after install: "Run `npm audit` for details"

## Why this matters for LLM agents

An LLM agent consuming summary output can autonomously drill down without needing a mental model of the full command tree. The output itself becomes self-documenting — the HATEOAS principle applied to CLI. This complements the [[backlog/done/vault-dashboard]] idea: the dashboard gives overview, and each piece of data tells you how to go deeper.

## Design considerations

- Text hints should be visually distinct (e.g., dimmed/grey `→` prefix) so they don't clutter the output
- Consider a `--no-hints` flag to suppress them for scripting
- JSON `links` field should use actual runnable command strings (not URL-style links)
- If `--dir` or `--glob` was passed, the hint commands should include those flags too
- Should work for: `properties summary`, `tags summary`, and the future vault dashboard/summary command
- For properties with common values, could also hint value-specific queries: `hyalo property find --name status --value "in-progress"`

## My Comments
The tricky part could be to know the "context" and decide on what actual commands to propose. How to design a good heuristic for this?

## Resolution

Implemented in [[iterations/done/iteration-11-discoverable-drill-down-commands]]. The heuristic question was solved by making hint generation state-aware — inspecting the actual output data rather than using static lookup tables. The `--hints` flag was chosen over `--no-hints` (opt-in rather than opt-out) to keep default output clean.
