---
type: iteration
title: "Iteration 255 — dogfood v0.22.0 remaining bugs: stale --index set, UTF-8 placeholder wording, new --property"
date: 2026-08-30
status: in-progress
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

### BUG-2: `set --index` no-op does not refresh a disk-changed entry [2/2]

- [x] Reproduce: append text to a note's body (no property change), then run
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
- [x] Add an e2e: create an index, externally append a body line, run a
      no-op `set --index`, assert the refreshed index reflects the disk
      state on the next `--index` query.

### UX-3: UTF-8 placeholder text is inconsistent with search behaviour [2/2]

- [x] `read <file-with-invalid-utf8>` shows a placeholder body claiming the
      file content is present but "lossy" (mojibake-safe) when in fact
      `find` skips the file from search entirely
      (`warning: skipping … stream did not contain valid UTF-8`). Reconcile
      the two: either make the placeholder say the file is excluded from
      search too, or change what the placeholder says to match whatever
      `read` actually renders for such a file. Read the current placeholder
      code and the `find`-skip code before deciding which one is easier to
      make honest; record the decision inline (no DEC needed unless the
      fix changes command behaviour visible outside error/placeholder text).
- [x] e2e: a scratch vault file with invalid UTF-8 bytes; assert `read`'s
      placeholder text and `find`'s skip warning describe the same fact.

### UX-5: `new --property` is undiscoverable [2/2]

- [x] `hyalo new --file foo.md --property status=draft` fails with
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
- [x] Whichever is chosen: update `new --help`'s EXAMPLES and, if a flag was
      added, the root `--help` COMMAND REFERENCE and `check-command-reference`
      gate output.

## Acceptance criteria

- [x] BUG-2 e2e passes and the underlying no-op-doesn't-refresh gap is
      closed (or, if investigation shows it is by design, record why as a
      DEC and downgrade this item to a doc fix in `rule-knowledgebase.md`).
- [x] UX-3's placeholder text and `find`'s skip warning agree on what
      happens to a non-UTF-8 file, verified by a new e2e.
- [x] UX-5 resolved by flag or help-pointer (decide, don't leave both
      undone); `new --help` reflects the choice.
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, all `xtask check-*`, `hyalo lint --strict`.

## Outcome

Shipped on `iter-255/dogfood-v0220-remaining-bugs`. Three decisions worth
recording, all taken inline rather than as DECs because none of them changes
what a command *does* beyond the bug being fixed.

**BUG-2 — fix shape: staleness repair on read, in the journal.** Confirmed the
plan's diagnosis: `set`/`append`/`remove` only call `MutationJournal::update_entry`
inside `if file_changed && !dry_run`, so a mutation that finds the property
already at its target value never touches the snapshot. The repair is a new
`MutationJournal::refresh_if_stale`, called once per file the command reads
(before the `--where-*` filter, so a filtered-out file is repaired too) and
gated on `!dry_run`. Staleness is `(mtime, size)` against the entry, both of
which the command already `stat`ed for its concurrent-write guard — so a batch
with nothing stale costs zero extra I/O, and the *size* half is what catches an
edit landing in the same second the index was built (the whole-second mtime
string would miss it). `size` is only consulted when the stored value is
non-zero: snapshots written before iter-252 default it to `0`, and treating
that as "changed" would rescan every file in an old index. The refresh itself
is a full rescan plus link-graph re-registration, via a new
`SnapshotIndex::refresh_entry_and_links_at` — the `_at` variant exists because
mutation commands hold a `full_path` that is not always `dir.join(rel_path)`
(symlinks, canonicalised prefixes). A file the index has never seen is left
alone: upserting is the *write* paths' guarantee, not a read's.

**UX-3 — text fix, not a behaviour fix.** The placeholder's "lossy in search"
claim is half-true, which is why it read as a lie: `find -e <regex>` really
does match such a line lossily (U+FFFD), while `find <text>` — the default BM25
path, which needs the body as a `String` — drops the whole file. Making BM25
lossy-read would have changed which files `find` returns, i.e. a DEC. Instead
both surfaces now print one shared constant,
`commands::INVALID_UTF8_CONSEQUENCE`, stating all three facts. A second e2e
checks each claim against real behaviour, so the two surfaces cannot agree on
something false.

**UX-5 — help-pointer, no new flag.** Per `feedback_no_cli_surface_growth` and
the [[decision-log]] bias, `new` keeps its schema-only scaffold. `run.rs` now
intercepts `--property`/`--tag` on `new` (the same interception pattern already
used for `find --filter` and `append --tag`) and answers with the
scaffold-then-`set` chain; `new --help` gained a PROPERTIES paragraph and the
chained example, and `rule-knowledgebase.md` says the same. Teaching the
`agent_discoverability` example gate to split a documented `a && b` chain into
its two argvs (rather than skip it, as it does pipelines) keeps both halves
under test.

## Non-goals

- Re-litigating UX-6/UX-7 (`find -- '--index …'` clap `--` handling) — a
  known clap parsing quirk with a tip already in place, not a hyalo bug.

## Links

- [[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]]
- [[iterations/iteration-254-dogfood-v0220-help-and-shape-fixes]]
