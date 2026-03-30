---
title: "Reduce hyalo-core pub surface — use pub(crate) for internals"
type: backlog
date: 2026-03-29
status: planned
origin: codebase review 2026-03-29
priority: low
tags:
  - api-design
  - refactor
---

## Problem

`hyalo-core` exposes all 14 modules as `pub mod` with no internal visibility boundaries. Implementation details like `detect_opening_fence`, `is_closing_fence`, `hyalo_options()`, `canonicalize_vault_dir` are unnecessarily public. External callers cannot distinguish stable API from internal helpers.

## Fix

- Mark internal helpers as `pub(crate)`: `detect_opening_fence`, `is_closing_fence`, `hyalo_options()`, `canonicalize_vault_dir`
- Consider making `ScannedIndex` and `SnapshotIndex` concrete types `pub(crate)` if external callers only need the `VaultIndex` trait
- Keep `VaultIndex`, `IndexEntry`, `LinkGraph`, frontmatter parse/write, and discovery as the public API surface

## Acceptance criteria

- [ ] No internal helpers are `pub` — only `pub(crate)`
- [ ] hyalo-cli still compiles (it's the only external consumer)
- [ ] All existing tests pass
