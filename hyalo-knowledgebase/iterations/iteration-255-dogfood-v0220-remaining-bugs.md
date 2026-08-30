---
type: iteration
title: "Iteration 255 — dogfood v0.22.0 remaining bugs: stale --index set, UTF-8 placeholder wording, new --property"
date: 2026-08-30
status: planned
tags:
  - iteration
  - dogfood-fixes
  - bugfix
branch: iter-255/dogfood-v0220-remaining-bugs
depends-on: "[[iterations/iteration-254-dogfood-v0220-help-and-shape-fixes]]"
---

# Iteration 255 — dogfood v0.22.0 remaining bugs

## Goal

Carry-over sweep from [[iterations/iteration-254-dogfood-v0220-help-and-shape-fixes]]
(PR #296): three concrete, still-open findings from
[[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]] that 254
explicitly scoped out because they are bugs/UX gaps, not the help-text and
result-shape coherence work 254 covered. All three were "STILL OPEN" in
254's own Bug Regression Testing table and named in its Goal's "Out of
scope" list.

## Tasks

### BUG-2: `set --index` no-op does not refresh a disk-changed entry [0/1]

- [ ] Reproduce: append text to a note's body (no property change), then run
      `set <file> --property status=completed --index` — reports 0
      modified because the property is already the target value on disk,
      but the in-memory/snapshot index entry for that file is not
      refreshed even though the file was read. Evidence from 254's dogfood
      pass: `find zzqqx --index --count` → 1 while a fresh (non-`--index`)
      `find` on disk → 2. Decide the fix shape: refresh the index entry for
      every file `set`/`append`/`remove` reads, independent of whether the
      write was a no-op — likely in the same
      `MutationJournal`-mediated path `check-mutation-journal` (ARCH-3,
      iter-226) already gates, since this is index-maintenance-on-mutation,
      not persistence.
- [ ] Add an e2e: create an index, externally append a body line, run a
      no-op `set --index`, assert the refreshed index reflects the disk
      state on the next `--index` query.

### UX-3: UTF-8 placeholder text is inconsistent with search behaviour [0/1]

- [ ] `read <file-with-invalid-utf8>` shows a placeholder body claiming the
      file content is present but "lossy" (mojibake-safe) when in fact
      `find` skips the file from search entirely
      (`warning: skipping … stream did not contain valid UTF-8`). Reconcile
      the two: either make the placeholder say the file is excluded from
      search too, or change what the placeholder says to match whatever
      `read` actually renders for such a file. Read the current placeholder
      code and the `find`-skip code before deciding which one is easier to
      make honest; record the decision inline (no DEC needed unless the
      fix changes command behaviour visible outside error/placeholder text).
- [ ] e2e: a scratch vault file with invalid UTF-8 bytes; assert `read`'s
      placeholder text and `find`'s skip warning describe the same fact.

### UX-5: `new --property` is undiscoverable [0/1]

- [ ] `hyalo new --file foo.md --property status=draft` fails with
      `error: unexpected argument '--property'` — `new` has no way to set
      properties beyond schema defaults at scaffold time, and `new --help`
      does not point the reader at `set` as the follow-up. Decide: either
      add `--property` to `new` (repeatable, same syntax as `set
      --property`, applied after the schema-default scaffold) or leave `new`
      defaults-only and add one `new --help` line + EXAMPLES entry chaining
      `new` then `set --property` in one command. Given [[decision-log]]'s
      general bias against growing the CLI surface without a clear payoff
      (see `feedback_no_cli_surface_growth` in project memory — killed
      `--iteration`, `--strict-index` on the same grounds), the
      help-pointer fix is the safer default; only add the flag if scaffold
      + immediate property set turns out to be a common real workflow.
- [ ] Whichever is chosen: update `new --help`'s EXAMPLES and, if a flag was
      added, the root `--help` COMMAND REFERENCE and `check-command-reference`
      gate output.

## Acceptance criteria

- [ ] BUG-2 e2e passes and the underlying no-op-doesn't-refresh gap is
      closed (or, if investigation shows it is by design, record why as a
      DEC and downgrade this item to a doc fix in `rule-knowledgebase.md`).
- [ ] UX-3's placeholder text and `find`'s skip warning agree on what
      happens to a non-UTF-8 file, verified by a new e2e.
- [ ] UX-5 resolved by flag or help-pointer (decide, don't leave both
      undone); `new --help` reflects the choice.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, all `xtask check-*`, `hyalo lint --strict`.

## Non-goals

- Re-litigating UX-6/UX-7 (`find -- '--index …'` clap `--` handling) — a
  known clap parsing quirk with a tip already in place, not a hyalo bug.

## Links

- [[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]]
- [[iterations/iteration-254-dogfood-v0220-help-and-shape-fixes]]
