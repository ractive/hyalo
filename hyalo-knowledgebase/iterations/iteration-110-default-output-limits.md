---
title: Default output limits for all list commands
type: iteration
date: 2026-04-14
status: in-progress
branch: iter-110/default-output-limits
tags:
  - cli
  - ux
  - llm
  - performance
  - dogfooding
---

# iter-110: Default output limits for all list commands

Large knowledgebases can produce output that busts an LLM's context window. Commands that return unbounded lists should have sensible default limits, with a "showing N of M" message when truncated and a way to request more.

## Affected commands

- `find` — no default limit today
- `lint` — no limit at all today
- `tags summary` — unbounded
- `properties summary` — unbounded
- `backlinks` — unbounded

`summary` already caps recent files to 10 via `--recent`.

## Design

- Add a default limit (e.g. 50 or 100) to each list command
- `--limit 0` means unlimited (change `parse_limit` to allow 0, treat as "no cap")
- When results are truncated, text output shows `showing N of M matches` (already works for `find`)
- JSON envelope always carries the real `total` so consumers know there's more
- The default can be overridden in `.hyalo.toml` (e.g. `default_limit = 100`)

## Tasks

- [x] Change `parse_limit` to accept 0 as "unlimited"
- [x] Add default limit to `find` (e.g. 50)
- [x] Add `--limit` to `lint` with default (e.g. 50)
- [x] Add `--limit` to `tags summary` with default
- [x] Add `--limit` to `properties summary` with default
- [x] Add `--limit` to `backlinks` with default
- [x] Support `default_limit` in `.hyalo.toml`
- [x] Ensure "showing N of M" message works consistently across all commands
- [x] Update e2e tests
