---
date: 2026-03-23
origin: dogfooding knowledgebase housekeeping + docs/content vault
priority: medium
status: completed
tags:
- backlog
- cli
- filtering
- ux
title: Property absence filter (--no-property / --property !K)
type: backlog
---

# Property absence filter (--no-property / --property !K)

## Problem

There is no way to find files that are *missing* a property. The `--property` flag supports existence checks (`--property status`) and value comparisons (`--property status=completed`), but not absence (`--property !status` or `--no-property status`).

This is needed for data quality audits: "which files are missing a `status`?", "which files have no `date`?"

## Proposed syntax

Option A: `--property '!status'` (negated existence)
Option B: `--no-property status` (separate flag)

Option A is more composable and consistent with the existing `!=` operator.

## Acceptance criteria

- [ ] Can filter for files missing a specific property
- [ ] Works in combination with other filters (AND semantics)
- [ ] Help text documents the syntax
- [ ] E2e tests cover absence filter
