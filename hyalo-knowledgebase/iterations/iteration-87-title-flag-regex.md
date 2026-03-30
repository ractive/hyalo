---
title: "Iteration 87: --title flag regex & warning improvements"
type: iteration
date: 2026-03-31
tags:
  - cli
  - ux
  - find
status: in-progress
branch: iter-87/title-flag-regex-warning
---

## Goal

Improve the `--title` flag UX so that `/regex/flags` delimited syntax works (matching `--property` behavior) and suspicious inputs that look like misused syntax trigger a helpful warning.

## Background

Dogfooding revealed that `--title '~=/^The /'` silently returns nothing because the `/…/` delimited regex syntax is only parsed by `--property` filters. The `--title` `~=` mode treats the remainder as a bare regex pattern, so the slashes become literal characters in the regex — which never matches and gives no feedback.

## Tasks

- [x] Make `--title '~=/pattern/flags'` parse delimited regex (reuse `parse_regex_pattern` logic)
- [x] Emit a warning when `--title` value looks like it's using `--property` syntax incorrectly
- [x] Add e2e tests for delimited regex and warning behavior
- [x] Run quality gates (fmt, clippy, test)
