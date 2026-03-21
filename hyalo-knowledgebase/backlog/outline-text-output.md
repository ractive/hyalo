---
date: 2026-03-21
origin: iteration-06 (open task)
priority: high
status: completed
tags:
- backlog
- outline
- cli
title: Outline text output as indented tree
type: backlog
---

# Outline text output as indented tree

## Problem

The `outline` command only outputs JSON. When an LLM or human wants a quick structural overview, the JSON is verbose and hard to scan. During iteration 6 dogfooding, we had to pipe JSON through python to get a readable tree view.

## Proposal

Implement `--format text` for the `outline` command. Output an indented tree:

```
iterations/iteration-06-outline.md
  title: Iteration 6 — Outline Command (in-progress)
  tags: iteration, outline, cli, llm
  # Iteration 6 — Outline Command
    ## Goal
    ## Motivation
    ## Relationship to Prior Work
      → [[iteration-02-links]], [[iteration-05-summary-list-refactor]]
    ## Design
      ### What the outline contains
      ### Output format [json]
      ### File targeting
    ## Tasks
      ### Core implementation [5/5 ✓]
      ### Typed structs refactor [3/3 ✓]
      ### Output formats [1/2]
      ### Code quality [4/4 ✓]
      ### Bug fixes [3/3 ✓]
      ### Documentation [5/5 ✓]
    ## Design Notes
```

Design choices:
- 2-space indent per heading level
- Task counts inline with completion indicator
- Links shown with `→` prefix
- Code block languages in brackets
- Frontmatter summary on first lines (title, status, tags)
- Multi-file mode: separate files with blank line

## Notes

This is the one remaining open task from iteration 6. Small scope — could be done as a standalone PR or bundled with another iteration.
