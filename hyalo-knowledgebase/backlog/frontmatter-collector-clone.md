---
title: FrontmatterCollector clones entire IndexMap per file
type: backlog
date: 2026-03-29
status: done
origin: codebase review 2026-03-29
priority: medium
tags:
  - performance
  - scanner
---

## Problem

`FrontmatterCollector::on_frontmatter` at `scanner.rs:737` does `self.props = props.clone()`. The multi-visitor API passes `&IndexMap` (shared reference across all visitors), so the collector must clone to take ownership. For vaults with complex frontmatter this is a significant per-file allocation.

## Possible fixes

1. Change scanner to pass ownership to the first visitor that claims it (breaking change to `FileVisitor` trait)
2. Have the scanner yield the parsed map after the visitor loop, letting the caller own it without cloning
3. Use `std::mem::take` if the scanner can be restructured to build the map in a swappable slot

## Acceptance criteria

- [ ] `FrontmatterCollector` no longer clones the full `IndexMap`
- [ ] Multi-visitor API still works (other visitors can read frontmatter)
- [ ] All existing tests pass
