---
title: Iteration 121 — Code Review Hardening
type: iteration
date: 2026-04-16
status: completed
branch: iter-121/review-hardening
tags:
  - iteration
  - security
  - code-quality
  - ux
  - docs
related:
  - "[[dogfood-results/dogfood-v0120-post-iter120]]"
---

# Iteration 121 — Code Review Hardening

## Goal

Address findings from the deep code review (post-iter-120 dogfood session):
two MEDIUM security issues in snapshot index handling, one missing `--dry-run`
flag, production `.expect()` cleanup, inconsistent error formatting, and
documentation gaps.

## Security Fixes

### SEC-1: Index `rel_path` traversal validation (MEDIUM)

Deserialized `IndexEntry::rel_path` values from `.hyalo-index` are accepted
verbatim. A crafted index with `rel_path = "../../etc/cron.d/malicious"` causes
`dir.join(rel_path)` to escape the vault when mutation commands trigger
`rescan_entry`.

**Fix**: In `SnapshotIndex::load_inner`, validate every `entry.rel_path` after
deserialization — reject (warn + fall back to disk scan) if any path has `..`
components, is absolute, or contains null bytes. Reuse `has_parent_traversal`
from `discovery.rs`.

- [x] Add `rel_path` validation in `load_inner` (`crates/hyalo-core/src/index.rs`)
- [x] Reject entire snapshot with warning if any path fails
- [x] Add unit test with crafted index containing traversal path
- [x] Add e2e test: index with `../../escape.md` entry is rejected

### SEC-2: Index deserialization OOM from crafted `.hyalo-index` (MEDIUM)

`rmp_serde::from_slice` applies no allocation limits. A crafted file with a
MessagePack array header claiming millions of entries triggers OOM.

**Fix**: Cap file size before reading. Check `metadata().len()` and reject files
above a reasonable limit (e.g. 512 MB) with a clear error.

- [x] Add file size check before `std::fs::read` in `SnapshotIndex::load`
- [x] Add test with oversized file size metadata

### SEC-3: `is_pid_alive` does not guard `pid == 0` (LOW)

A crafted snapshot with `pid = 0` causes `kill(0, 0)` which checks the process
group, not a specific process — always returns alive, preventing stale index
cleanup.

- [x] Add `pid == 0` guard in `is_pid_alive` (`crates/hyalo-core/src/index.rs`)

## Code Quality

### CQ-1: `task set` missing `--dry-run`

Only mutation command without `--dry-run`. `task toggle` has it; `task set`
(same kind of write) does not.

- [x] Add `--dry-run` flag to `TaskAction::Set` in `args.rs`
- [x] Thread through dispatch and `task_set` implementation
- [x] Add e2e test for `task set --dry-run`

### CQ-2: `types.rs` `.expect()` in production (~30 instances)

TOML table access uses `.expect("schema is a table")` after `toml_type_exists`
guard. Safe by invariant but panics on hand-edited `.hyalo.toml` with wrong
structure.

- [x] Replace `.expect()` calls with `.context()` + `?` returning user error
- [x] Add test: malformed `.hyalo.toml` (e.g. `schema = "string"`) gives error not panic

### CQ-3: `types.rs`/`views.rs` bypass `format_error()` (~12 instances)

Use raw `format!("Error: ...")` instead of `format_error()`, so errors don't
respect `--format text` vs JSON.

- [x] Add `format: Format` parameter to `views.rs` functions
- [x] Replace `format!("Error: ...")` with `format_error()` in types.rs and views.rs
- [x] Verify with `--format text` that errors render correctly

## Documentation

### DOC-1: Update `--fields` help text to mention `outline` alias

- [x] Add `(alias: outline)` next to `sections` in help text (`args.rs:234`)

### DOC-2: Update README

- [x] Add `--stemmer`/`--language` section with ISO 639-1 examples
- [x] Add `--dry-run` examples for `properties rename` and `tags rename`
- [x] Mention `outline` alias in `--fields` examples

### DOC-3: Update CHANGELOG

- [x] Add `skipped` → `skipped_count` breaking change for `properties rename` and `tags rename` JSON output

### DOC-4: Update skill template

- [x] Mention ISO 639-1 codes in `skill-hyalo.md` `--stemmer` section

## Quality Gates

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace -q`

## Acceptance Criteria

- [x] Crafted `.hyalo-index` with traversal `rel_path` is rejected with warning
- [x] Crafted `.hyalo-index` over 512 MB is rejected
- [x] `task set --dry-run` previews without writing
- [x] `types remove` on malformed `.hyalo.toml` returns error, not panic
- [x] `views list --format text` errors are format-aware
- [x] README documents `--stemmer`, `--dry-run` for rename, `outline` alias
- [x] CHANGELOG documents `skipped_count` breaking change
- [x] All existing tests still pass
