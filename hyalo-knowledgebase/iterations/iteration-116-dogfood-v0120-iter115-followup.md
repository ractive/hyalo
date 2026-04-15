---
title: Iteration 116 — Dogfood v0.12.0 iter-115 Follow-up Fixes
type: iteration
date: 2026-04-15
status: completed
branch: iter-116/dogfood-v0120-iter115-followup
tags:
  - dogfooding
  - bug-fix
  - ux
  - task
related:
  - "[[dogfood-results/dogfood-v0120-iter115-followup]]"
  - "[[iterations/iteration-115-dogfood-v0120-iter114-followup]]"
---

# Iteration 116 — Dogfood v0.12.0 iter-115 Follow-up Fixes

## Goal

Close the one PARTIAL / NEW issue raised by the iter-115 dogfood report. The four
other bugs and five other UX items from iter-115 are already verified FIXED and
do not need further work.

## In scope

### UX-3 / NEW-1: `task toggle --dry-run` arrow direction (LOW)

**Problem**: The text output of `task toggle --dry-run` shows the *post-toggle*
state only: `"tasks.md":6 [ ] Completed task`. At a glance this reads as
"this task is currently unchecked" rather than "this task will be flipped from
checked to unchecked". Iter-115 added the arrow format in `tasks.rs` but the
`Format::Text` branch was unreachable — the dispatch layer forces JSON internally
and text rendering happens later via shape-based jq filters in the output
pipeline.

**Fix**:
- Introduce `TaskDryRunResult { file, line, old_status, status, text, done }` in
  `hyalo-core` and emit it (instead of `TaskReadResult`) from the dry-run branch
  of `task_toggle`. The new shape carries the pre-toggle status explicitly.
- Register a new text filter keyed on the `done,file,line,old_status,status,text`
  signature that renders `"file":line [old] -> [new] text`.
- Drop the unreachable `Format::Text` branch from `task_toggle`; text rendering
  is now driven entirely by the pipeline filter, which matches every other
  command in the codebase.

**Tests**: e2e `task_toggle_dry_run_json_includes_old_status` and
`task_toggle_dry_run_text_uses_arrow_format`.

## Out of scope

### BUG-6 / NEW-2: MDN absolute URL-style links remain unresolved

Verified against a synthesized vault: absolute links like
`/en-US/docs/Web/JavaScript/Iteration` **do** resolve and **do** get rewritten by
`mv` once the vault root and `site_prefix` are aligned (prefix `en-US/docs`,
vault containing `Web/JavaScript/Iteration.md`). The MDN failure mode in the
dogfood report is really case-sensitivity: MDN's on-disk layout is lowercase
(`web/javascript/iteration.md`) while the links use PascalCase
(`/en-US/docs/Web/JavaScript/Iteration`). On macOS's default case-insensitive
APFS this appears to resolve but the reported `path` field has the wrong case;
on Linux it would not resolve at all.

This reduces BUG-6 to the same root cause as UX-5 (old) — link resolution
case-sensitivity — which was already tagged LOW and deferred. Addressing it
cleanly requires threading a case-insensitive file index into
`resolve_target`/`LinkGraph` and is a standalone iteration rather than a
follow-up fix.

## Tasks

- [x] Add `TaskDryRunResult` to `hyalo-core::types`
- [x] Emit `TaskDryRunResult` from dry-run branch of `task_toggle`
- [x] Add `TASK_DRY_RUN_RESULT_FILTER` and register it in `lookup_filter`
- [x] Add e2e tests for JSON `old_status` field and text arrow format
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace -q` (607/607 pass)

## Acceptance Criteria

- [x] `hyalo task toggle <file> --all --dry-run --format text` emits
  `"file":line [old] -> [new] text` for every toggled task
- [x] `hyalo task toggle <file> --all --dry-run --format json` includes both
  `old_status` and `status` on every result
- [x] File on disk is unchanged after `--dry-run`
- [x] All existing task tests still pass
