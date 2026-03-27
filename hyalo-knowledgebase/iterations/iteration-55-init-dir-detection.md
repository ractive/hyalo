---
title: "Replace hardcoded hyalo-knowledgebase in init --claude and add fuzzy dir detection"
type: iteration
date: 2026-03-27
tags:
  - init
  - cli
  - developer-experience
status: in-progress
branch: iter-55/init-dir-detection
---

## Problem

`hyalo init --claude` installs skill templates that contain hardcoded references to
`hyalo-knowledgebase/`. When users have a different docs directory (e.g. `docs/`,
`my-knowledgebase/`), the installed skills reference a non-existent path.

Additionally, auto-detection only checks exact directory names from a fixed list
(`docs`, `knowledgebase`, `wiki`, etc.), missing common variants like `my-docs` or
`project-knowledgebase`.

## Tasks

- [x] Parameterize skill templates with actual dir value (like rule template already does)
- [x] Add fuzzy directory detection (substring matching on candidate names)
- [x] Update agent template to not hardcode hyalo-knowledgebase
- [x] Add unit tests for fuzzy detection and template parameterization
- [x] Add e2e tests verifying skills are parameterized
- [ ] Create PR and review
