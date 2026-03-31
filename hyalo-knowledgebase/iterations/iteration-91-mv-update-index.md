---
title: "Iteration 91: mv command — full index update (including link graph)"
type: iteration
date: 2026-03-31
tags:
  - index
  - mv
  - refactor
status: in-progress
branch: iter-91/mv-update-index
---

## Goal

Make the `mv` command fully update the snapshot index when `--index` is passed, including
the link graph and outbound links on affected files. Currently mv only patches `rel_path`
and `modified` on the moved entry — backlink/link queries go stale. Copilot flagged this
in PR #101 review comment.

Also refactor mv's index logic to use shared `mutation.rs` helpers (consistency with
set/remove/append), and update README.md to remove the caveat.

## Tasks

- [x] Add `LinkGraph::rename_path()` method (`crates/hyalo-core/src/link_graph.rs`)
- [x] Add unit test for `rename_path`
- [x] Make `scan_one_file` `pub` and `FileLinks` `pub` (`crates/hyalo-core/src/index.rs`, `link_graph.rs`)
- [x] Add `SnapshotIndex::graph_mut()` method
- [x] Add `SnapshotIndex::rescan_entry()` + `refresh_entry()` methods
- [x] Add `mutation::rename_index_entry()` helper (`crates/hyalo-cli/src/commands/mutation.rs`)
- [x] Refactor `mv.rs` index update to use `mutation::rename_index_entry` + `save_index_if_dirty`
- [x] Extend e2e test `mv_with_index_updates_index_path` to verify backlinks after mv
- [x] Update README.md — remove mv index caveat
- [x] Run fmt + clippy + tests

## Design notes

- `LinkGraph::rename_path(old_rel, new_rel)` handles both stem and `.md` key variants
- `FileLinks` promoted from `pub(crate)` to `pub` so `scan_one_file` can be public
- `rescan_entry` stays `pub(crate)`; public callers use `refresh_entry` (no `FileLinks` in signature)
- The rewrite plans from `plan_mv` tell us exactly which files had links rewritten — re-scan
  only those files (plus the moved file itself) to update their `entry.links` field
- Performance: re-scanning a handful of files is negligible; mv already reads them all during planning
