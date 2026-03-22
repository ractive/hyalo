---
date: 2026-03-21
origin: dogfooding
priority: low
status: completed
tags:
- backlog
- cli
- ux
title: Add --glob filter to properties and tags aggregate commands
type: backlog
---

# Add --glob filter to properties and tags aggregate commands

## Problem

The `properties` and `tags` commands (aggregate/summary mode) always scan the entire vault. There is no `--glob` option to limit the scan to a subfolder or file pattern, unlike `property find` and `tag find` which do support `--glob`.

This makes it hard to get a quick overview of just a subset of files (e.g. `backlog/*.md` or `iterations/*.md`).

## Discovered during

Dogfooding: after moving completed backlog items to `backlog/done/`, wanted to quickly list statuses of only the remaining `backlog/*.md` files using `properties`, but the command doesn't accept `--glob`.

## Proposal

Add `--glob` and `--file` options to `properties` and `tags` commands, consistent with the existing filter options on `property find` / `tag find`.
