---
title: "Panic on empty --status value"
type: backlog
date: 2026-03-29
status: planned
origin: codebase review 2026-03-29
priority: high
tags:
  - bug
  - cli
---

## Problem

`main.rs:1659` — `status.chars().next().unwrap()` panics if a user passes `--status ""`. Should return a user-facing error instead.

## Fix

Validate non-empty before accessing first char, or use `.ok_or_else(|| anyhow!("--status must not be empty"))`.

## Acceptance criteria

- [ ] `--status ""` returns a user error, not a panic
- [ ] E2e test covers this case
