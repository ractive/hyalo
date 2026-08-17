---
title: Iteration 195 — review drain cleanups
type: iteration
date: 2026-08-17
status: in-progress
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

- [x] Remove `pub fn lint_files` from `commands/lint.rs` and port its single
      unit-test caller (`lint.rs:3662`) to call `lint_files_with_options`
      with `FixMode::Off` directly. Do NOT touch `FixMode`,
      `lint_files_with_options`, or `lint_files_extended` — they are live.
- [x] Grep for any other zero-caller `pub` items in `commands/lint.rs`
      surfaced by the removal (clippy `-D warnings` plus
      `cargo +stable build` will flag newly-dead private items; for `pub`
      items check callers manually). Remove what is genuinely dead, list
      anything intentionally kept in the PR body.

### Windows correctness [0/2]

- [x] ~~Extend the guard at `discovery.rs:1390` to
      `!target.contains('/') && !target.contains('\\')`~~ — **not done, and
      correctly so.** Verified during implementation: the finding is a false
      positive. `resolve_target` opens with an unconditional
      `target.replace('\\', "/")` and only truncates/trims between there and
      the guard, so `target` provably cannot contain a backslash at line 1390.
      Adding the check would be a permanently-false condition. Instead: a
      comment at the guard explaining why it differs from its three siblings
      (they inspect *raw* link targets), plus DEC-066. See Results.
- [x] Unit test: a target containing `'\\'` does not enter bare-stem
      resolution — landed as
      `resolve_target_backslash_targets_are_normalized_before_stem_resolution`,
      pinning the *actual* invariant: `note.md\` normalizes to `note.md` and
      resolves as a bare name, and `sub\other.md` resolves as a path; neither
      is ever truncated to a mangled stem such as `note.` (which the test
      proves is otherwise resolvable). Platform-independent.

### CLI flag consistency (non-breaking, aliases only) [0/3]

- [x] `create-index`: add `--path` as a hidden-or-visible alias for
      `--output`; `drop-index`: add `--output` as an alias for `--path`.
      Long aliases only — do not add second short flags. Update COMMAND
      REFERENCE if the alias is made visible (the iter-192
      `check-command-reference` gate and hint-execution gate must stay
      green).
- [x] `-n` divergence (`--limit` vs `--recent`): do NOT change flag
      semantics. Document the divergence where it bites: one sentence in
      `summary --help` for `-n/--recent` noting it differs from
      `find -n/--limit`.
- [x] `links fix` vs `links auto` naming: decide alias-or-document and
      record the decision in the decision log (DEC-0NN). Default position:
      document only — a `visible_alias` here adds surface without removing
      the inconsistency.

### KB hygiene [0/1]

- [x] Fix the MD012 double blank line at
      `iterations/iteration-188-link-semantics-completion.md:134`
      (via `hyalo lint --fix` scoped to that file, or a one-line edit).

## Acceptance criteria

- [x] `grep -rn "lint_files(" crates --include='*.rs'` shows no callers of
      the removed wrapper (and the wrapper is gone)
- [x] The `discovery.rs` bare-stem guard is correct and its divergence from
      the three sibling guards is explained in-code and in DEC-066; the new
      unit test is platform-independent and passes on all three CI platforms
      (revised from "tests both separators" — see Results)
- [x] `hyalo create-index --path X` and `hyalo drop-index --output X` both
      work; existing spellings unchanged; hint-execution and
      check-command-reference gates green
- [x] `hyalo lint --strict` on the KB reports 4 warnings (the deliberate
      HYALO002 quartet in 152/159/173/181) and nothing else
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean
- [x] Review file [[reviews/codebase-review-2026-08-06]] gains a short
      "Dispositions" section mapping every finding/observation to the
      iteration that landed it (191/192/193/195) or an explicit won't-fix

## Non-goals

- Anything touching the mdbook-lint `convert_fix` workarounds — that is
  [[iteration-196-mdlint-workaround-strip]], gated on an upstream release.
- Renaming or breaking any existing flag or subcommand.
- The `links auto` persistent-exclusions backlog item
  (`backlog/auto-link-config-exclusions.md`) stays in the backlog.

## Results (2026-08-17)

### Dead code — two wrappers removed, not one

- `commands::lint::lint_files` deleted; its single unit-test caller
  (`lint_no_schema_no_violations`) now calls `lint_files_with_options` with
  `FixMode::Off` directly.
- The zero-caller sweep found a **second** leftover of the same kind:
  `commands::lint::prepend_file_result` (72 lines) had no callers anywhere in
  the workspace — not even a test. It was superseded by
  `inject_ext_file_result` when dispatch moved to the extended lint path
  (`dispatch.rs:2021`). Deleted.
- Intentionally kept, all with live in-module callers: `FixMode`,
  `lint_files_with_options`, `lint_files_extended`, `lint_counts_only`
  (called from `commands/types.rs`), `validate_views` (dispatch),
  `validate_constraint_simple`, and the `VIOLATION_KIND_*` / `RULE_ID_*`
  constants and serialization structs (`FileFixResult`, `FixedGroup`,
  `ConflictEntry`, `RuleGroup`, `BodyViolation`, `ExtLintOutput`, …).

### Windows correctness — finding does not reproduce

The review's `discovery.rs` observation is a false positive; see DEC-066 for
the full reasoning and [[reviews/codebase-review-2026-08-06]] for the
disposition. Summary: backslash normalization happens ~100 lines *before* the
guard, so the mangled-stem path is unreachable. The planned code change would
have been dead defence that reads as real. Landed instead: an explanatory
comment at the guard naming why its three siblings differ, DEC-066, and a
regression test pinning the normalization invariant.

### CLI flag consistency

- `create-index` gained `--path` and `drop-index` gained `--output`, both as
  clap `visible_alias`es, so `-h` shows `[aliases: --path]` / `[aliases:
  --output]`. No short flag added, no spelling removed. COMMAND REFERENCE
  annotates both lines. Two e2e tests exercise the aliases end-to-end
  (`create_index_accepts_path_alias_for_output`,
  `drop_index_accepts_output_alias_for_path`), plus a help test asserting the
  aliases stay discoverable.
- `-n` divergence documented, not changed: a FLAG NOTE in `summary`'s
  `long_about` and a note on the `--recent` arg itself. The note names only
  `find` and `backlinks` — verified with `-h` that `links`, `tags`, and
  `properties` have no `-n` at all, so the review's "find-family" framing
  would have been inaccurate.
- `links fix` / `links auto`: documented only, as DEC-065.

### KB hygiene

`hyalo lint --fix --file iterations/iteration-188-link-semantics-completion.md`
fixed the MD012. `hyalo lint --strict` on the KB now reports exactly the
4 deliberate HYALO002 warnings (152/159/173/181) and nothing else.

### Also re-checked

`deny.toml`'s two RUSTSEC ignores (a Round 1 observation asking for a re-check
after any `mdbook-lint` bump): `cargo tree -i bincode` after iter-193's 0.15.2
bump still shows `bincode 1.3.3 -> syntect 5.3.0 -> comrak 0.21.0`, reached
from both `hyalo-mdlint` and `mdbook-lint-core`. Both ignores stay.
