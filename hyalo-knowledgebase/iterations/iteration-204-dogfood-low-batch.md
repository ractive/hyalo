---
title: Iteration 204 — dogfood v0.21.0-pre medium/low batch
type: iteration
date: 2026-08-18
status: planned
branch: iter-204/dogfood-low-batch
tags:
  - iteration
  - cleanup
  - cli
related:
  - "[[dogfood-results/dogfood-v0210-pre-release-iters-191-198]]"
---

# Iteration 204 — dogfood v0.21.0-pre medium/low batch

## Goal

Batch-clear the remaining MEDIUM and actionable LOW findings from the
v0.21.0-pre dogfood that are not covered by iterations 200-203: CI
footguns, envelope inconsistencies, doc drift, and small correctness
debts. Each item names its report ID; the report holds the full repro.

**Do NOT release; release is a separate user-gated step.**

## Context

All findings and exact repros:
[[dogfood-results/dogfood-v0210-pre-release-iters-191-198]]. This plan
intentionally stays a checklist — items are independent and small; an
item that turns out deep gets split out rather than ballooning here.

## Tasks

### CI footguns [0/3]

- [ ] M-10: `lint --rule <name>` validates the rule id (same lookup
      `lint-rules show` uses) — unknown or wrong-case id is exit 1 with
      the `lint-rules list` hint, not silent 0 findings. Accept
      case-insensitive match. Same for `--rule-prefix` matching nothing:
      warn.
- [ ] L-2: a write command whose single explicitly named (non-glob) file
      was skipped (unparseable) exits 1; message states nothing was
      modified. Glob/batch behavior unchanged.
- [ ] M-6: `find --index` staleness signal — compare index file mtime vs
      vault dir mtime (cheap, both already stat-ed) and warn on stderr
      `index older than vault; results may be stale — re-run
      create-index` (suppressed by `--no-hints`? No: warnings are not
      hints; suppressed only by explicit future flag if demanded).
      Document the snapshot contract in create-index help.

### Envelope/output consistency [0/4]

- [ ] L-5: `read` honors piped-JSON default on ALL error paths (missing
      file, dir target, bad --lines, --count rejection) — emit the same
      JSON error envelope as siblings.
- [ ] L-6: `find -e` regex errors go through the standard error envelope
      (JSON under --format json / piped) and report the pattern as typed
      — strip the internal `(?i)` from user-facing text (the
      `--property 'title~=/…/'` path is the model).
- [ ] L-10: `create-index` text output includes `files_indexed` and the
      "replaced existing index" note (JSON parity, --help already
      promises it).
- [ ] L-15: standardize match positions to 1-based line AND column in
      JSON output; CHANGELOG breaking note (consumers may parse col).

### Hint/message fixes [0/3]

- [ ] L-9: the `drop-index` hint after `create-index -o <custom>` carries
      `--path <custom>`; extend the hint-execution gate fixture to a
      custom-path index so the pairing is gate-checked.
- [ ] L-7: `drop-index` on a nonexistent in-vault path reports file-not-
      found (with the path), not a boundary-check failure with an
      irrelevant `--allow-outside-vault` hint.
- [ ] UX-2: `read --section` not-found error truncates the heading list
      to the 5 closest matches (existing fuzzy machinery) + "and N more —
      run `hyalo read <file>` to list sections".

### Doc drift (surface truth residue) [0/3]

- [ ] M-8: fix the global `--help` limit-contract paragraph to name only
      commands that actually cap at 50 and accept `--limit`; either give
      `types list`/`views list`/`lint-rules list` a `--limit` or remove
      them from the claim (removal preferred — small lists). Decide bare
      `hyalo tags`/`properties` flag handling: either accept the summary
      flags (preferred: they already alias to summary) or fix COMMAND
      REFERENCE to stop promising it. Kill the BUG-3 residue
      (`hyalo tags --limit 0` must work or must not be documented).
- [ ] M-9: rewrite the `mv` COMMAND REFERENCE entry: positional
      FILE/DEST form, batch mode (--glob/--property/--tag/--type +
      --apply), --allow-ambiguous, --on-conflict. Consider extending
      check-command-reference to assert every clap flag of a subcommand
      appears in its entry (drift gate upgrade — if too noisy, record
      why in the PR body).
- [ ] L-13(b): document `exclude_target_globs` case-insensitivity in
      links auto help + configuration.md.

### Small correctness debts [0/4]

- [ ] L-1: `backlinks` stops double-counting case-mismatched wikilinks
      (register under the canonical target only; `find --fields links`
      is the reference behavior).
- [ ] L-4: `mv` refuses to clobber a dangling symlink at DEST (exists-
      check via symlink_metadata, message suggests removing it first).
- [ ] L-16: covered in iter-202 (exit-code unification) — verify here
      only that `okf log` now exits 1; if 202 has not merged first,
      do it here and tell 202's agent via the plan.
- [ ] iter-203 follow-up: site-prefix stripping (`strip_site_prefix`) is
      case-sensitive and tries only the single auto-derived guess (last
      path component of `--dir`). MDN's derived prefix (`en-us`) never
      matches its real two-segment, mixed-case URL prefix
      (`en-US/docs`), so every site-absolute link (`/en-US/docs/...`)
      stays unresolved until `--site-prefix "en-US/docs"` is passed by
      hand — `hyalo config` now at least surfaces the derived guess
      (iter-203 UX-4) so this is discoverable, but nothing resolves
      automatically. Make the match case-insensitive; decide whether
      auto-derivation should also try, or `hyalo config` should warn
      about, a multi-segment prefix. Add a regression fixture shaped
      like MDN (vault dir `en-us`, links written `/en-US/docs/...`).

## Acceptance criteria

- [ ] Every ticked item has a unit or e2e test reproducing the report's
      scenario
- [ ] `hyalo lint --rule MD0133` exits 1; `--rule hyalo006` finds what
      `--rule HYALO006` finds
- [ ] All error paths of `read` and `find -e` emit JSON when piped
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- L-3 (chmod 444 rewrite, hardlink breakage) and L-8 (index
  reproducibility) — recorded as accepted behavior unless a user report
  arrives; note them in the report's disposition instead.
- L-11 (mv link normalization) — folded into iter-203's mv task.
- UX-3 (`links` text output ordering) and the `links` perf profiling —
  separate design-worthy task, not a batch item.
