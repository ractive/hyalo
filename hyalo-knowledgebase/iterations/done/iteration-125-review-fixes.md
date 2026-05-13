---
title: "Iteration 125 — Post-review fixes: correctness, security, docs, index consistency"
type: iteration
date: 2026-04-22
tags:
  - iteration
  - bugfix
  - security
  - docs
status: completed
branch: iter-125/review-fixes
---

## Goal

Address all findings from the consolidated review of iterations 123–124 (auto-link feature). Covers a correctness bug, security hardening, performance improvements, documentation gaps, and index consistency.

## Context

Three parallel reviews (Rust best practices, security audit, documentation) surfaced 14 findings across critical/high/medium/low severity. This iteration fixes the critical and high items, the most impactful medium items, and the documentation gaps.

## Critical / High

### 1. Fence ordering bug in `link_rewrite.rs`

**Bug:** `plan_inbound_rewrites` (line ~405) and `plan_outbound_rewrites` (line ~580) check `is_comment_fence` before `fence.process_line`. A `%%` line inside a fenced code block incorrectly toggles comment mode, causing all subsequent lines to be skipped.

**Fix:** Mirror the correct ordering from `auto_link.rs:472-485` — check `fence.process_line` first, then only check comment fences when `!fence.in_fence()`.

- [x] Fix fence ordering in `plan_inbound_rewrites`
- [x] Fix fence ordering in `plan_outbound_rewrites`
- [x] Add test: `%%` inside a fenced code block must not toggle comment mode

### 2. `apply_matches` missing vault-boundary check

**Bug:** `apply_matches` in `auto_link.rs:553-585` writes files via `atomic_write` without calling `ensure_within_vault`. The equivalent write path in `execute_plans` (link_rewrite.rs:326) does this correctly.

**Fix:** Call `canonicalize_vault_dir` once before the loop, then `ensure_within_vault` for each file before `atomic_write`.

- [x] Add `ensure_within_vault` check in `apply_matches` before each `atomic_write`

### 3. `apply_matches` double-read without mtime check

**Bug:** Files are read during the scan phase, then read again in `apply_matches`. Between reads, concurrent edits can cause silent mismatches. Replacements are silently skipped via the guard at line 573, but the user sees no warning.

**Fix:** Retain the content read during scanning and pass it through to `apply_matches`, eliminating the second read entirely. This also improves performance (no double I/O).

- [x] Refactor to pass scanned content through to `apply_matches` (eliminate double-read)
- [x] Emit a warning to stderr when a replacement is skipped due to content mismatch

## Medium

### 4. Index not updated by `links fix`, `links auto --apply`, `lint --fix`

**Bug:** Three mutating commands write to disk but don't patch the snapshot index. Subsequent `--index` queries see stale data.

| Command | Writes disk | Updates index |
|---------|-------------|---------------|
| `links fix --apply` | Yes | No |
| `links auto --apply` | Yes | No |
| `lint --fix` | Yes | No |

**Fix:** Thread `snapshot_index` through the dispatch layer for all three commands and call `mutation::update_index_entry` after each file write, then `mutation::save_index_if_dirty` at the end. Mirror the pattern used by `set`, `remove`, `append`.

- [x] Thread `snapshot_index` into `links fix` dispatch and update index after each file write
- [x] Thread `snapshot_index` into `links auto --apply` dispatch and update index after each file write
- [x] Thread `snapshot_index` into `lint --fix` dispatch and update index after each file write

### 5. `--file` arg not validated for path traversal

**Bug:** The `--file` argument for `links auto` is not checked for `..` components or absolute paths before use.

**Fix:** Reject `--file` arguments containing `..` components or leading `/` at the CLI entry point, using `has_parent_traversal` from `discovery.rs`.

- [x] Add path traversal validation for `--file` in links auto dispatch

### 6. `exclude_lower` is O(n) Vec — should be HashSet

**Performance:** `auto_link.rs:110-113` builds a `Vec<String>` of excluded titles, then `.contains()` is called per title insertion — O(entries × exclude_titles).

- [x] Change `exclude_lower` from `Vec<String>` to `HashSet<String>`

### 7. Options struct for `auto_link` / `links_auto`

**Code quality:** The main function has 9 parameters hidden behind `#[allow(clippy::too_many_arguments)]`.

- [x] Extract an `AutoLinkOptions` struct to replace the 9 parameters
- [x] Apply the same pattern to `links_auto` CLI dispatch if applicable

## Documentation

### 8. No cookbook entries for `links auto` in root `--help`

**Gap:** The COOKBOOK section in `HELP_LONG` (help.rs) has 40+ recipes but zero for `links auto`.

- [x] Add 3-4 cookbook entries for `links auto` to `HELP_LONG` in `crates/hyalo-cli/src/cli/help.rs`

### 9. README auto-link section is minimal

**Gap:** README.md has two bare example lines for auto-link, no explanatory text.

- [x] Expand README auto-link section with a brief description, use cases, and more examples

### 10. Common mistakes not documented in help text

**Gap:** The help text doesn't document common mistakes (e.g. `--exclude-target-glob` vs `--exclude-title` confusion, ambiguous title handling).

- [x] Add COMMON MISTAKES section to `links auto` help text, consistent with `find` subcommand

## Acceptance Criteria

- [x] `%%` inside a code fence does NOT toggle comment mode in `link_rewrite.rs`
- [x] `apply_matches` calls `ensure_within_vault` before every write
- [x] No double-read of files in the apply path
- [x] `links fix --apply`, `links auto --apply`, and `lint --fix` all patch the snapshot index
- [x] `--file` with `..` or absolute path is rejected with a clear error
- [x] `exclude_lower` is a `HashSet`
- [x] Root `--help` cookbook includes `links auto` recipes
- [x] README has an explanatory auto-link section
- [x] `links auto --help` includes a COMMON MISTAKES section
- [x] `cargo fmt` passes
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [x] `cargo test --workspace -q` passes
