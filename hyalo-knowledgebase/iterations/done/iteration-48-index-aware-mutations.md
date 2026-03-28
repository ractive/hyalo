---
branch: iter-48/index-aware-mutations
date: 2026-03-26
status: completed
tags:
- iteration
- index
- mutations
- performance
title: Iteration 48 — Index-Aware Mutations
type: iteration
---

# Iteration 48 — Index-Aware Mutations

## Goal

When `--index` is passed, mutation commands (`set`, `remove`, `append`, `task`, `mv`,
`tags rename`, `properties rename`) should use the index for file discovery **and** patch the
in-memory index entry after mutating, then save the updated snapshot back to disk. No disk
re-read of the mutated file — each command already has the new state in memory.

## Design

### What changes per mutation

| Command | Index fields to patch |
|---|---|
| `set` | `properties`, `tags` (re-derive), `modified` |
| `remove` | `properties`, `tags` (re-derive), `modified` |
| `append` | `properties`, `tags` (re-derive), `modified` |
| `task` | `tasks` (toggle status), `sections` (update task counts in heading), `modified` |
| `mv` | `rel_path`, `modified`, link graph (rewrite targets) |
| `tags rename` | `properties`, `tags`, `modified` |
| `properties rename` | `properties`, `tags` (if tags property renamed), `modified` |

### What stays unchanged

- `sections` (except task-count suffixes on headings for `task` command)
- `links` (outbound links don't change from frontmatter mutations)
- `link_graph` (except `mv` which changes link targets vault-wide)

### Architecture

- Make `SnapshotIndex` mutable: add methods to update/replace an `IndexEntry` by path and to
  re-save atomically
- Each mutation command receives `Option<&mut SnapshotIndex>` — if `Some`, patch + save after
  the file write succeeds
- Tag re-derivation reuses the existing `extract_tags()` helper from the frontmatter module
- `mv` is the most complex: must update both the entry's `rel_path` and all link graph entries
  pointing to the old path

## Tasks

- [x] Add `update_entry(&mut self, rel_path: &str, entry: IndexEntry)` to `SnapshotIndex`
- [x] Add `remove_entry(&mut self, rel_path: &str)` to `SnapshotIndex` (for `mv`)
- [x] Add `insert_entry(&mut self, entry: IndexEntry)` to `SnapshotIndex` (for `mv`)
- [x] Add `save(&self)` method to `SnapshotIndex` that re-serializes and atomically writes
- [x] Refactor mutation commands to accept `Option<&mut SnapshotIndex>`
- [x] `set`: after file write, patch `properties`/`tags`/`modified` in index and save
- [x] `remove`: after file write, patch `properties`/`tags`/`modified` in index and save
- [x] `append`: after file write, patch `properties`/`tags`/`modified` in index and save
- [x] `task`: after file write, patch `tasks`/`sections`/`modified` in index and save
- [x] `mv`: after file write, update `rel_path`, link graph entries, and save
- [x] `tags rename`: after file write, patch `properties`/`tags`/`modified` and save
- [x] `properties rename`: after file write, patch `properties`/`tags`/`modified` and save
- [x] Update CLI dispatch in `main.rs` to pass `SnapshotIndex` to mutation commands when `--index` is set
- [x] E2e test: `set` with `--index`, then `find --index` sees updated property
- [x] E2e test: `remove` with `--index`, then `find --index` confirms removal
- [x] E2e test: `task` with `--index`, then `find --index` sees toggled task
- [x] E2e test: `mv` with `--index`, then `find --index` sees new path and updated links
- [x] E2e test: chained mutations with `--index` — multiple mutations keep index consistent
- [x] Update skill documentation and README to reflect that mutations now support `--index`
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`

## Acceptance Criteria

- [x] All mutation commands accept `--index` and update the snapshot in-place
- [x] No disk re-read after mutation — index patched from in-memory state only
- [x] Index file atomically re-saved after each mutation
- [x] Chained `--index` mutations produce identical index to a fresh `create-index`
- [x] All quality gates pass
