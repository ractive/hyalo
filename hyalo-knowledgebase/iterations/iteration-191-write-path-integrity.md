---
title: Iteration 191 — write-path integrity (symlinks, durability, dead mutators)
type: iteration
date: 2026-08-06
status: completed
branch: iter-191/write-path-integrity
tags:
  - iteration
  - write-path
  - correctness
  - data-integrity
related:
  - "[[reviews/codebase-review-2026-08-06]]"
  - "[[dogfood-results/dogfood-v0200-opus5-review-round]]"
---

# Iteration 191 — write-path integrity

## Goal

Close the three defects in `fs_util::atomic_write` and its callers found by
the 2026-08-06 review. This is the one iteration in the set that fixes
**silent data loss**, so it ships alone and first — do not fold the CLI
surface work (iter-192) or the dependency work (iter-193) into this branch.

All four items live in or immediately around a single 30-line function that
every mutation path funnels through. That is what makes them one unit.

**Do NOT release; release is a separate user-gated step.**

## Context

`atomic_write` is called by `set`, `remove`, `append`, `task`, `lint --fix`,
`mv`, `okf`, `changelog`, `managed_region`, and `link_rewrite`. Every finding
below therefore affects all of them at once.

Verified before planning (do not re-litigate):

- The **out-of-vault** symlink case is already correctly rejected —
  `hyalo task toggle` on a symlink pointing outside the vault errors with
  `file resolves outside vault boundary`. Only the **intra-vault** case is
  broken. Do not "fix" the boundary check.
- The missing fsync in `write_snapshot` (`index.rs:919`) is **benign** — a
  torn snapshot fails `rmp_serde::from_slice` and falls back to a disk scan.
  Leave it, or change it only as an explicit non-goal note. The user-markdown
  path is the one with no recovery.
- `links fix` bucket accounting is correct and was re-verified in the
  2026-08-06 dogfood. Nothing in this iteration should touch it.

## Tasks

- [x] Decide and record the symlink policy in `decision-log.md` — follow the
      link and write the target, **or** refuse with a clear error. Recommend
      *follow*: refusing breaks vaults that legitimately alias notes, and
      Obsidian follows links. Record as a DEC entry before writing code.
- [x] Implement the chosen policy in `atomic_write` (`fs_util.rs:17-51`) —
      resolve `path` via `fs::canonicalize` (or `read_link` loop with a depth
      cap) before choosing the rename destination, so `persist` lands on the
      real file rather than replacing the link.
- [x] Guard the resolved destination with the existing vault-boundary check
      so following a link cannot escape the vault — a symlink to
      `../../etc/passwd.md` must still be refused, not followed.
- [x] Add `tmp.as_file().sync_all()` before `persist` in `atomic_write`, and
      fsync the parent directory afterwards on Unix so the rename itself is
      durable.
- [x] Correct the `atomic_write` doc comment so its durability claim matches
      what the code now actually guarantees.
- [x] Delete `tasks::toggle_task` and `tasks::set_task_status` (`tasks.rs:481`,
      `:521`) — zero callers, and they lack the `check_mtime` guard their
      plural counterparts have. If they must stay for semver, add the guard
      instead and say why in the doc comment.
- [x] Harden `apply_body_fixes` (`lint.rs:3087,3092`) — replace
      `result[start..end]` with `result.get(start..end)` and treat `None` as
      `FixOutcome::Conflict`, covering `start > end` and non-char-boundary
      offsets that the current `end > body.len()` check misses.
- [x] Delete the dead no-op loop at `lint.rs:1689-1694` and its
      self-contradicting comment.
- [x] Update CHANGELOG under `Fixed` — the symlink item is user-visible
      behaviour change and needs to be called out explicitly.

## Acceptance criteria

- [x] e2e: `hyalo task toggle` on an intra-vault symlink updates the link
      **target** and leaves the symlink a symlink — test name
      `task_toggle_follows_intra_vault_symlink`
- [x] e2e: `hyalo lint --fix` on a symlinked file is idempotent — second run
      reports zero violations — test name `lint_fix_through_symlink_is_idempotent`
- [x] e2e: a symlink whose target escapes the vault is still refused with
      `file resolves outside vault boundary` — test name
      `symlink_escaping_vault_is_refused`
- [x] e2e: `hyalo set` / `hyalo append` / `hyalo mv` each verified against a
      symlinked file — one test per command, no shared fixture shortcuts
- [x] unit: `atomic_write` calls `sync_all` before `persist` — asserted
      structurally or via a doc test, since durability itself is not
      unit-testable
- [x] unit: `apply_body_fixes` returns `Conflict` (does not panic) for a
      `DiagFix` with `start > end` and for one with a mid-UTF-8-char `end` —
      test names `apply_body_fixes_rejects_inverted_range` and
      `apply_body_fixes_rejects_non_char_boundary`
- [x] `grep -rn "toggle_task\b\|set_task_status\b" crates/` returns only the
      plural forms (or the singular forms show a `check_mtime` call)
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean
