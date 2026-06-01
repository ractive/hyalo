---
title: Iteration 152 — Reconcile frontmatter write vs. parse size budget
type: iteration
date: 2026-06-01
status: planned
branch: iter-152/frontmatter-size-budget
tags:
  - iteration
  - frontmatter
  - parser
  - bug
related:
  - "[[dogfood-results/dogfood-v0160-iter-149-creative]]"
  - "[[dogfood-results/dogfood-v0160-iter-150-crazy]]"
---

## Goal

Close BUG-3 from
[[dogfood-results/dogfood-v0160-iter-149-creative]] (still open in the
iter-150 round): the write path accepts frontmatter values that the
read path then refuses to parse, silently orphaning the file from every
hyalo command.

```
hyalo set f.md --property "huge=$(python3 -c 'print("x"*10000)')"
# exit 0, JSON envelope confirms write
hyalo find --file f.md
# warning: skipping f.md: frontmatter too large (no closing `---` found
#          within 200 lines / 8192 bytes)
# No results
```

## Why now

This is a data-integrity bug. A successful `set` is the user's signal
that their data is safe in the vault. Today that signal is wrong: the
file becomes invisible to every read path until the user notices and
hand-trims the frontmatter. Severity HIGH from the iter-149 dogfood,
unchanged after iter-150.

## Scope

### IN

**1. Pick a single budget and enforce it on both sides.**

The current read-side budget is 200 lines / 8192 bytes (in the
frontmatter scanner). The write side has no budget at all. Options:

- **Option A (recommended):** raise the read-side budget to something
  generous (e.g. 64 KiB / 2000 lines) and have the write path enforce
  the *same* number with a clear error if exceeded.
- **Option B:** keep the read-side small (8 KiB) and have `set`
  refuse writes that would produce frontmatter exceeding it.

A picks "be permissive about reading what users wrote." B picks
"frontmatter is metadata, keep it small." Recommendation: **A**, but
the implementing agent should confirm with a quick check of how MDN /
GitHub Docs frontmatter sizes distribute in practice (`hyalo find
--jq` over wc of `properties`). If 99th-percentile real-world
frontmatter fits under 8 KiB by a wide margin, B is fine too.

**2. Symmetric error.**

Whichever budget is chosen, the write side must:

- Reject the write upfront with a structured error:
  ```json
  {
    "error": "frontmatter would exceed size budget",
    "limit_bytes": 65536,
    "would_be_bytes": 71234,
    "file": "f.md"
  }
  ```
- Exit non-zero. No partial write. No silent truncation.

**3. Read-side warning becomes one-time + actionable.**

Today the "frontmatter too large" warning prints on every command that
touches the file. Make it diagnose once per `create-index` (or once
per command run for unindexed scans) and include `hyalo lint <file>`
as the suggested next step.

### OUT

- General YAML schema validation tightening.
- Pretty-printing huge frontmatter values in `find` output (truncation
  is fine).
- Other write/read asymmetries (covered separately if discovered).

## Tasks

- [ ] Sample real-world frontmatter sizes across MDN + own KB, decide
      A vs B, note in PR
- [ ] Apply chosen budget to the frontmatter scanner constant
- [ ] Add the same budget check to the `set` / `append` / `remove`
      write path before the file is written
- [ ] Add the same check to `hyalo new`
- [ ] Emit structured error on over-budget write, non-zero exit
- [ ] De-duplicate the parse warning (once per file, not once per
      command)
- [ ] Suggest `hyalo lint <file>` in the warning text
- [ ] Test: write a 10 KB property, assert refusal + non-zero exit +
      structured error fields
- [ ] Test: write a value just under the budget, assert success +
      read round-trip
- [ ] Test: write a value just over, assert refusal
- [ ] Test: existing files with over-budget frontmatter still produce
      a (de-duplicated) parse warning, not a crash
- [ ] Update `set` / `append` / `new` `--help` with the budget number
- [ ] `cargo fmt && cargo clippy --workspace --all-targets -- -D
      warnings && cargo test --workspace -q` clean
- [ ] Mark `status=completed`, move to `iterations/done/`

## Acceptance Criteria

- [ ] Write path and read path agree on a single frontmatter size
      budget, documented in `--help`.
- [ ] `hyalo set` refuses to write frontmatter that would exceed the
      budget; exit non-zero; structured error.
- [ ] A file written with the largest *allowed* frontmatter parses
      cleanly on read and round-trips in `find` / `read` / `lint`.
- [ ] The iter-149 BUG-3 repro (`set --property "huge=$(printf 'x'
      x 10000)"`) returns a structured error at write time, not a
      silent success followed by parse failure.
- [ ] The parse warning is emitted at most once per file per command
      run (not per scanned line).
- [ ] No regression on existing tests.

## Notes for the implementing agent

- The repro is in [[dogfood-results/dogfood-v0160-iter-149-creative]]
  BUG-3 — copy that scenario verbatim into the test suite as
  `bug_iter149_3_frontmatter_size_budget`.
- Resist the urge to make the budget configurable on a per-file basis.
  One number for the whole tool is fine; it can be raised later if
  users actually push against it.
- Check whether the budget interacts with `--max-frontmatter-lines`
  or similar existing knobs before introducing a new constant.
