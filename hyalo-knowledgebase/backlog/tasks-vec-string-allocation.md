---
title: "Task mutation allocates N strings to modify one line"
type: backlog
date: 2026-03-29
status: planned
origin: codebase review 2026-03-29
priority: low
tags:
  - performance
  - tasks
---

## Problem

`tasks.rs:473,513` — `lines.iter().map(|l| l.to_string()).collect::<Vec<String>>()` allocates a `Vec<String>` from `Vec<&str>` just to mutate one line. For a 500-line file, that's 500 heap allocations.

## Fix

Rewrite with string slicing: concatenate the lines before the target, the modified line, and the lines after, without converting all lines to owned Strings.

## Acceptance criteria

- [ ] Task mutation doesn't allocate per-line Strings
- [ ] All task e2e tests pass
