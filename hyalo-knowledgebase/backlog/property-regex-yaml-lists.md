---
title: Property regex doesn't match inside YAML list values
type: backlog
tags:
  - find
  - frontmatter
  - regex
  - dogfooding
status: planned
date: 2026-03-30
priority: medium
---

## Problem

`--property 'status~=deprecated'` regex only matched 2 of 591 deprecated pages — it doesn't match inside YAML list values.

## Workaround

Use `--property 'status'` (existence check) combined with `--jq` post-filter to inspect list contents.

## Enhancement

Extend property regex matching to search inside YAML list/array values, not just scalar strings.
