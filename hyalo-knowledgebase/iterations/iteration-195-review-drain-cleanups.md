---
title: Iteration 195 — review drain cleanups
type: iteration
date: 2026-08-17
status: planned
branch: iter-195/review-drain-cleanups
tags:
  - iteration
  - cleanup
  - cli
related:
  - "[[reviews/codebase-review-2026-08-06]]"
---

# Iteration 195 — review drain cleanups

## Goal

Drain the last actionable leftovers of the Opus 5 review
([[reviews/codebase-review-2026-08-06]]) so nothing from it remains
unaddressed and undocumented. Everything here is small, low-risk and mostly
mechanical: one dead-code removal that slipped between iterations, one
Windows-only correctness wrinkle, non-breaking CLI flag aliases, and one
KB formatting nit. After this iteration the review is fully dispositioned:
every finding and observation either landed as code or is explicitly
recorded as won't-fix.

**Do NOT release; release is a separate user-gated step.**

## Context

Verified at `93d6491` (2026-08-17):

- `lint_files` (`crates/hyalo-cli/src/commands/lint.rs:184`) is a thin
  wrapper over `lint_files_with_options` with **zero production callers** —
  the only caller is a unit test at `lint.rs:3662`. Dispatch uses
  `lint_files_extended` exclusively. Flagged during iter-191's review round
  ("removal candidate, fits iter-192") but iter-192 never picked it up.
  Note: `FixMode` itself is **live** (dispatch.rs:1820-1825) — only the
  wrapper is dead.
- `crates/hyalo-core/src/discovery.rs:1390`: the bare-stem resolution guard
  is `!target.contains('/')` while the three sibling separator checks in the
  same file (lines 830, 1006, 1136) test both `'/'` **and** `'\\'`. On
  Windows a pathological target like `note.md\` slips past the guard, gets
  stem `note.` and misses the lookup — wrong answer, not a panic. From the
  review's Round 1 observations (untracked).
- CLI flag pairs from the review's Round 2 observations (untracked):
  `create-index` uses `-o/--output` (args.rs:961) while `drop-index` uses
  `-p/--path` (args.rs:979) for the *same file*; `-n` means `--limit` on
  `find`-family commands (args.rs:353 et al.) but `--recent` on `summary`
  (args.rs:621); `links fix` / `links auto` pair a verb with an adjective.
- `iterations/iteration-188-link-semantics-completion.md:134` has an MD012
  warning (two consecutive blank lines) — the only non-deliberate lint
  finding left in the KB.

## Tasks

### Dead code [0/2]

- [ ] Remove `pub fn lint_files` from `commands/lint.rs` and port its single
      unit-test caller (`lint.rs:3662`) to call `lint_files_with_options`
      with `FixMode::Off` directly. Do NOT touch `FixMode`,
      `lint_files_with_options`, or `lint_files_extended` — they are live.
- [ ] Grep for any other zero-caller `pub` items in `commands/lint.rs`
      surfaced by the removal (clippy `-D warnings` plus
      `cargo +stable build` will flag newly-dead private items; for `pub`
      items check callers manually). Remove what is genuinely dead, list
      anything intentionally kept in the PR body.

### Windows correctness [0/2]

- [ ] Extend the guard at `discovery.rs:1390` to
      `!target.contains('/') && !target.contains('\\')`, matching the three
      sibling sites in the same file.
- [ ] Unit test: a target containing `'\\'` does not enter bare-stem
      resolution (assert the lookup misses rather than resolving a mangled
      stem). The test must not depend on running on Windows — it exercises
      the guard, not the filesystem.

### CLI flag consistency (non-breaking, aliases only) [0/3]

- [ ] `create-index`: add `--path` as a hidden-or-visible alias for
      `--output`; `drop-index`: add `--output` as an alias for `--path`.
      Long aliases only — do not add second short flags. Update COMMAND
      REFERENCE if the alias is made visible (the iter-192
      `check-command-reference` gate and hint-execution gate must stay
      green).
- [ ] `-n` divergence (`--limit` vs `--recent`): do NOT change flag
      semantics. Document the divergence where it bites: one sentence in
      `summary --help` for `-n/--recent` noting it differs from
      `find -n/--limit`.
- [ ] `links fix` vs `links auto` naming: decide alias-or-document and
      record the decision in the decision log (DEC-0NN). Default position:
      document only — a `visible_alias` here adds surface without removing
      the inconsistency.

### KB hygiene [0/1]

- [ ] Fix the MD012 double blank line at
      `iterations/iteration-188-link-semantics-completion.md:134`
      (via `hyalo lint --fix` scoped to that file, or a one-line edit).

## Acceptance criteria

- [ ] `grep -rn "lint_files(" crates --include='*.rs'` shows no callers of
      the removed wrapper (and the wrapper is gone)
- [ ] The `discovery.rs` guard tests both separators; new unit test passes
      on all three CI platforms
- [ ] `hyalo create-index --path X` and `hyalo drop-index --output X` both
      work; existing spellings unchanged; hint-execution and
      check-command-reference gates green
- [ ] `hyalo lint --strict` on the KB reports 4 warnings (the deliberate
      HYALO002 quartet in 152/159/173/181) and nothing else
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean
- [ ] Review file [[reviews/codebase-review-2026-08-06]] gains a short
      "Dispositions" section mapping every finding/observation to the
      iteration that landed it (191/192/193/195) or an explicit won't-fix

## Non-goals

- Anything touching the mdbook-lint `convert_fix` workarounds — that is
  [[iteration-196-mdlint-workaround-strip]], gated on an upstream release.
- Renaming or breaking any existing flag or subcommand.
- The `links auto` persistent-exclusions backlog item
  (`backlog/auto-link-config-exclusions.md`) stays in the backlog.
