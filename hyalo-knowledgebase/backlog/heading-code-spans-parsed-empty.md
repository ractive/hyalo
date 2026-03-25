---
title: "Heading text inside code spans parsed as empty string"
type: backlog
date: 2026-03-25
origin: dogfooding v0.3.1 against docs/content
priority: low
status: planned
tags: [backlog, bug, scanner, sections]
---

# Heading text inside code spans parsed as empty string

## Problem

Headings like `` ### `versions` `` are parsed with an empty heading string `""`. The backtick/code span content is stripped. In text output, these appear as bare `### ` lines.

The code span text should be preserved as the heading text (e.g., `versions`).

## Repro

Find files with code-span headings in docs/content and check `--fields sections` output.

## Acceptance criteria

- [ ] Headings containing inline code spans preserve the code text
- [ ] `--fields sections` shows the actual heading text
- [ ] E2e test with a code-span heading
