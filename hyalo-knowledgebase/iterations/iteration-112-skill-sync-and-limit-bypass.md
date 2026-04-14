---
title: "Iteration 112 — Sync Skills with Recent Features, Bypass Default Limit for --jq/--count"
type: iteration
date: 2026-04-14
status: in-progress
branch: iter-112/skill-sync-and-limit-bypass
tags:
  - skills
  - ux
  - schema
---

## Goal

Bring the two Claude Code skill templates (hyalo and hyalo-tidy) up to date with
features added since iter-32, and fix a silent data truncation issue where `--jq`
and `--count` pipelines received only 50 results due to the default limit.

## Tasks

- [x] Schema: auto-add string property for required fields without explicit definitions
- [x] Default limit bypass for `--jq` and `--count` (programmatic pipelines need complete data)
- [x] Add `show` alias for `read` command
- [x] Skill hyalo.md: document `--title`, `--sort`, `--broken-links`, `read` examples, views with BM25 patterns
- [x] Skill hyalo-tidy.md: add schema/lint phases, dead-end detection
- [x] Remove `--limit 0` from tidy skill (no longer needed)
- [x] Update help texts to document limit bypass behaviour
- [x] E2e tests for jq/count limit bypass, show alias, schema auto-string
- [x] Address review: fix help text wording, stale `types create` ref, add tags bypass test
- [ ] Update README with limit bypass and show alias
- [ ] Create iteration file
