---
title: "Unified VaultIndex: eliminate duplicate vault scans"
type: iteration
date: 2026-03-29
tags:
  - performance
  - refactor
  - architecture
status: in-progress
branch: iter-68/unified-vault-index
---

## Goal

Make every read-only command go through `VaultIndex` — whether backed by a snapshot or a fresh `ScannedIndex`. Eliminate all duplicate `discover_files` walks and duplicate file scans. The disk-scan variants (`find`, `summary`, `backlinks`, `properties_summary`, `tags_summary`, `links_fix`) become dead code and are removed.

## Context

Today, when no snapshot index exists, each command runs its own ad-hoc scan loop with hand-picked visitors. Some commands (`find` with backlinks, `summary` with `--glob`) scan the vault 2–3 times because they call `LinkGraph::build` or `discover_files` separately. The `_from_index` variants already exist and work correctly — they just aren't used in the non-snapshot path.

**Key insight:** `ScannedIndex::build(files, site_prefix)` already does exactly what the ad-hoc scan loops do — runs all visitors, builds the `LinkGraph`, produces `IndexEntry` structs. We just need to call it unconditionally and route through `_from_index`.

**Scoping rule for `--glob`:**
- If a command needs full-vault data (backlinks, link health, orphans), scan the full vault
- Otherwise, scan only `--glob`-matched files
- The decision is known at dispatch time from the CLI flags

**Body-skip optimization:** `ScannedIndex::build` currently always scans file bodies (sections, tasks, links). But several commands only need frontmatter:

| Command | Needs body? | Why |
|---|---|---|
| `find --property status=planned` (no sections/tasks/links/backlinks) | No | Only properties + tags for filtering |
| `properties summary` | No | Only properties |
| `tags summary` | No | Only tags |
| `find` with sections/tasks/links fields or filters | Yes | Sections, tasks, links from body |
| `find --fields backlinks` | Yes | Links needed for link graph |
| `summary` | Yes | Tasks for counts, links for graph |
| `backlinks` | Yes | Links for link graph |
| `links fix` | Yes | Links for resolution |

## Design

### `ScanOptions` — control what `ScannedIndex::build` scans

Add a `ScanOptions` struct to `hyalo-core` that `ScannedIndex::build` accepts:

```rust
pub struct ScanOptions {
    /// When false, only frontmatter is read — sections, tasks, and links
    /// fields in IndexEntry will be empty Vecs. The LinkGraph will be empty.
    pub scan_body: bool,
}
```

When `scan_body = false`, `scan_one_file` skips the `SectionScanner`, `TaskExtractor`, and `LinkGraphVisitor` — only `FrontmatterCollector::new(false)` is used, so the scanner stops after the YAML frontmatter delimiter. No body bytes are read from disk.

The caller determines `scan_body` at dispatch time based on the command and its flags. This preserves the current `find()` behavior where `body_needed` is computed from the field/filter combination.

### Dispatch flow

```
main.rs dispatch:
  1. Determine needs_full_vault (backlinks, summary, links fix, backlinks cmd)
  2. Determine needs_body (sections, tasks, links, task filters, content search, broken links)
  3. let index = snapshot_index.unwrap_or_else(||
       build_scanned_index(dir, files_arg, globs, site_prefix,
                           needs_full_vault, ScanOptions { scan_body: needs_body })
     );
  4. command_from_index(&index, ...)
```

### `needs_body` determination per command

For `find`:
```rust
let needs_body = fields.sections || fields.tasks || fields.links || fields.backlinks
    || fields.title || has_section_filter || has_task_filter || has_title_filter
    || pattern.is_some() || regexp.is_some() || broken_links || sort_needs_links;
```
(This mirrors the existing `body_needed` computation in `find.rs:91-102`.)

For `summary`: always true (needs task counts + link graph).
For `backlinks`: always true (needs link graph).
For `links fix`: always true (needs links).
For `properties summary`: always false.
For `tags summary`: always false.

## Tasks

### Phase 1: Add `ScanOptions` to `ScannedIndex::build`

- [x] Add `ScanOptions` struct in `hyalo-core/src/index.rs`
- [x] Change `ScannedIndex::build` signature to accept `&ScanOptions`
- [x] Change `scan_one_file` to accept `scan_body: bool` — when false, only use `FrontmatterCollector::new(false)` and skip section/task/link visitors
- [x] When `scan_body = false`, produce empty `sections`, `tasks`, `links` vecs and skip `LinkGraph::from_file_links` (use `LinkGraph::default()` or empty)
- [x] Update `create-index` command to pass `ScanOptions { scan_body: true }` (snapshot always needs full data)
- [x] Verify all existing tests still pass

### Phase 2: Add `build_scanned_index` helper

- [x] Add `build_scanned_index(dir, files_arg, globs, format, site_prefix, needs_full_vault, scan_options) -> Result<ScannedIndexBuild>` in `commands/mod.rs`
- [x] When `needs_full_vault`: call `discover_files(dir)` for all vault files
- [x] When not `needs_full_vault`: call `collect_files(dir, files_arg, globs, format)` for only matching files
- [x] Call `ScannedIndex::build(files, site_prefix, &scan_options)` and return the result

### Phase 3: Unify dispatch in main.rs

- [x] For `find`: compute `needs_full_vault` and `needs_body`, replace `if snapshot { find_from_index } else { find }` with unified path through `build_scanned_index` + `find_from_index`
- [x] For `summary`: `needs_full_vault = true`, `needs_body = true`, replace dispatch
- [x] For `backlinks`: `needs_full_vault = true`, `needs_body = true`, replace dispatch
- [x] For `properties summary`: `needs_full_vault = false`, `needs_body = false`, replace dispatch
- [x] For `tags summary`: `needs_full_vault = false`, `needs_body = false`, replace dispatch
- [x] For `links fix`: `needs_full_vault = true`, `needs_body = true`, replace dispatch

### Phase 4: Remove dead disk-scan functions

- [x] Remove `find()` (disk-scan variant) from `find.rs`
- [x] Remove `summary()` (disk-scan variant) from `summary.rs`
- [x] Remove `backlinks()` disk-scan variant from `backlinks.rs`
- [x] Remove `properties_summary()` disk-scan variant from `properties.rs`
- [x] Remove `tags_summary()` disk-scan variant from `tags.rs`
- [x] Remove `links_fix()` disk-scan variant from `links.rs` (if it exists)
- [x] Remove the standalone `LinkGraph::build(dir, site_prefix)` call in `find.rs:135-143`
- [x] Clean up unused imports, dead helpers, and now-unreachable code paths

### Phase 5: Rename `_from_index` → clean names

- [x] Rename `find_from_index` → `find`
- [x] Rename `summary_from_index` → `summary`
- [x] Rename `backlinks_from_index` → `backlinks`
- [x] Rename `properties_summary_from_index` → `properties_summary`
- [x] Rename `tags_summary_from_index` → `tags_summary`
- [x] Update all call sites in `main.rs`

### Phase 6: Verify and gate

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] Manual dogfood: run `hyalo find`, `hyalo summary`, `hyalo find --fields backlinks`, `hyalo summary --glob "iterations/*"`, `hyalo find --property status=planned`, `hyalo tags summary`, `hyalo properties summary` against `hyalo-knowledgebase/` — verify identical output to current main

## Non-goals

- **Mutation commands** (`set`, `remove`, `append`, `mv`, `task`, `tags rename`, `properties rename`) are not changed — they operate on single files and patch the snapshot index in-place. They don't have the multi-scan problem.
- **Body content search optimization** — `find_from_index` already re-reads files only when needed for content search. No change required.
- **Snapshot index format changes** — `IndexEntry` already contains all needed data. No schema changes.

## Risks

- The `_from_index` variants may have subtle behavior differences from the disk-scan variants. Mitigation: the extensive e2e test suite (864+ tests) will catch regressions.
- Commands that don't need body data will get `IndexEntry`s with empty `sections`/`tasks`/`links` vecs. If any code assumes these are populated, it will silently produce incomplete output. Mitigation: the `needs_body` flag mirrors the existing `body_needed` logic that already gates these fields.
- `ScanOptions { scan_body: false }` produces an empty `LinkGraph`. Code that calls `index.link_graph()` on a body-skipped index would get no backlinks. Mitigation: `needs_full_vault = true` always implies `scan_body = true` (backlinks require links from body).

## Expected impact

- **Performance:** `find --property status=planned` without snapshot: reads only YAML frontmatter per file (no body parsing). `summary --glob "iterations/*"`: one vault scan instead of three. `find --fields backlinks`: one vault scan instead of two.
- **Code reduction:** ~500-800 lines of duplicate scan logic removed (6 disk-scan functions + their helpers).
- **Maintainability:** One query code path per command instead of two. Bug fixes and features only need to be implemented once.

## References

- [[backlog/find-limit-memory-optimization]] — related performance work
- Codebase review (2026-03-29) identified duplicate vault scans as the #1 and #2 critical performance issue
