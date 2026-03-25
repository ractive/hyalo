---
title: "--hints flag silently accepted but does nothing for find/properties/tags"
type: backlog
date: 2026-03-25
origin: dogfooding v0.3.1
priority: low
status: planned
tags: [backlog, ux, cli, hints]
---

# --hints flag silently accepted but does nothing for most commands

## Problem

`--hints` works for `summary` but produces no hints for `find`, `properties summary`, or `tags summary`. The flag is silently accepted without effect.

Either these commands should generate useful drill-down hints, or `--hints` should warn/error when used with commands that don't support it.

## Proposal

Option A (preferred): Add meaningful hints to `find`, `properties summary`, and `tags summary`.
Option B: Reject `--hints` on commands that don't support it.

## Acceptance criteria

- [ ] `--hints` either produces output or warns on commands that don't support it
