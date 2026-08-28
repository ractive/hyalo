---
title: >-
  Iteration 246 — help coherence fixes and read UTF-8 diagnostic (deep-review
  follow-ups)
type: iteration
date: 2026-08-28
tags:
  - iteration
  - ux
  - help
  - review
status: completed
branch: iter-246/help-coherence-review-followups
related:
  - "[[reviews/deep-review-2026-08-27]]"
  - "[[iterations/iteration-245-deferral-carryovers]]"
---

# Iteration 246 — help coherence fixes and read UTF-8 diagnostic

## Goal

Fix every actionable finding from [[reviews/deep-review-2026-08-27]]: the
hand-maintained COMMAND REFERENCE in `cli/help.rs` has drifted from the real
clap surface (F-1..F-4), `hyalo read` misreports invalid UTF-8 as an
oversized-line (F-5), the README overclaims `--dry-run` coverage (F-6), and
no test mechanically ties help prose or hint coverage to the real CLI (F-7).

Non-goals: the large-file refactor of `hints.rs`/`lint.rs`/`output.rs`, and
the S-2/S-3 stale-index design questions (warn-but-serve is a deliberate
trade-off; record, don't change). `reviews/` using the undeclared
`type: review` is a vault-schema decision for the owner, not code.

## Context

- The COMMAND REFERENCE is a hand-written string template
  (`HELP_LONG_TEMPLATE` in `crates/hyalo-cli/src/cli/help.rs:46`). There is
  no mechanical check against the clap surface, so it drifts — this iteration
  fixes the current drift AND adds the guard that prevents recurrence.
- The hint execution gate (`crates/hyalo-cli/tests/e2e/hint_execution.rs`)
  already proves harvested hints run; it just doesn't seed changelog/okf/madr.
- Existing pattern for a byte-accurate help regression test:
  `crates/hyalo-cli/tests/e2e/help.rs` (see
  `summary_help_documents_short_n_divergence`, `options_block`,
  `examples_block` helpers).

## Tasks

### F-5: read UTF-8 misdiagnosis (code fix — do first, it's the only real bug)

- [x] Change `read_line_capped` in `crates/hyalo-core/src/scanner/mod.rs:522`
      to distinguish invalid UTF-8 from over-quota truncation. Replace the
      `bool` in the `(usize, bool)` return with a small public enum, e.g.
      `LineOutcome { Complete, Truncated, InvalidUtf8 }` (keep the `usize`
      byte count). The invalid-UTF-8 arm at scanner/mod.rs:589-597 currently
      returns `truncated = true`; make it return `InvalidUtf8`. Both
      `Truncated` and `InvalidUtf8` still drain to the next newline and
      advance the reader — only the reported cause differs.
- [x] Update the two call sites in
      `crates/hyalo-cli/src/commands/read.rs:212` and `:236`: on
      `InvalidUtf8` push a new placeholder
      `<line skipped: invalid UTF-8 (lossy in search; fix encoding to read)>`
      — wording bikesheddable, must not mention the MiB limit. Keep
      `oversized_line_placeholder()` for the real over-quota case.
- [x] Grep for other `read_line_capped` callers (only scanner internals +
      read.rs today) and update unit tests in
      `crates/hyalo-core/src/scanner/mod.rs` (`mod tests` at :744 — the
      `read_line_capped_*` tests around :1753-1776) for the new enum; add a
      test: a chunk containing `0xFF` returns `InvalidUtf8`, not `Truncated`.
- [x] Add e2e test in `crates/hyalo-cli/tests/e2e/` (extend an existing
      read test file or `errors.rs`): file with a `\xff` body byte →
      `hyalo read <file>` output contains the invalid-UTF-8 placeholder and
      does NOT contain "per-line limit".
- [x] CHANGELOG entry under `### Fixed`.

### F-1: summary `--limit` phantom flag

- [x] In `crates/hyalo-cli/src/cli/help.rs:74`, change the summary synopsis
      from `hyalo summary [-g/--glob G] [-n/--recent N] [--depth N] [--limit N]`
      to `hyalo summary [-g/--glob G] [-n/--recent N] [--depth N]`
      (drop `[--limit N]` — `summary` has no `--limit`; its own subcommand
      help FLAG NOTE already says so).

### F-2/F-3: changelog and okf synopses

- [x] help.rs:127: `hyalo changelog add <CATEGORY> <TEXT>` →
      `hyalo changelog add --category <CAT> --message "..."` (verify against
      `hyalo changelog add --help` Usage line:
      `hyalo changelog add [OPTIONS] --category <CATEGORY> --message <TEXT>`).
- [x] help.rs:128: `hyalo changelog release <VERSION>` is CORRECT (positional
      VERSION is real) — leave unchanged.
- [x] help.rs:132: `hyalo okf log <TEXT> [--apply]` →
      `hyalo okf log --message <TEXT> [TARGET] [--apply]`.

### F-4: missing flags in reference synopses

- [x] help.rs:86 (links fix): extend to include `[--apply-fuzzy]`,
      `[--min-confidence F]`, `[--case-insensitive]`, `[--expand-short-form]`.
      Keep it one line if possible; wrap to a continuation line matching the
      existing style if not.
- [x] help.rs:77-79 (task): add a note line under the three synopses:
      `--line accepts comma-separated lists; --section H and --all select
      tasks without line numbers` (both `--section` and `--all` are real on
      task read/toggle/set).
- [x] help.rs:48-50 (find): add `[--language LANG]` to the find synopsis.

### F-7: mechanical guards (the recurrence prevention)

- [x] Add e2e test `help_reference_synopses_parse` (new file
      `crates/hyalo-cli/tests/e2e/help_reference.rs` or extend `help.rs`):
      extract every `hyalo <...>` line from the COMMAND REFERENCE section of
      `hyalo --help`, strip synopsis-only notation (`[...]`, `<...>`,
      `K=V` placeholders → concrete benign values, e.g. `status=x`,
      `N` → `1`, `T` → `x`), and assert each resulting argv **parses**
      (clap accepts it; run against a temp vault with `--dry-run` where the
      command mutates, or just assert "unexpected argument" /
      "required arguments" errors are absent — parse-level only, not
      semantic success). This is the test that would have caught F-1..F-4.
      If full synopsis-to-argv compilation proves fiddly, the accepted
      fallback is a table of (subcommand, flag) assertions for the specific
      flags in F-1..F-4 plus a comment pointing here.
- [x] Extend `SEED_COMMANDS` in
      `crates/hyalo-cli/tests/e2e/hint_execution.rs:112` with seeds for
      `changelog` (needs a `CHANGELOG.md` with an `## [Unreleased]` section
      in the fixture), `okf index`, `okf log --message x`, and `madr toc`
      (fixture needs the profile-appropriate layout — check
      `tests/e2e/madr_profile.rs` / `okf_profile.rs` for minimal fixtures to
      copy). Harvested hints from these must then execute cleanly, same as
      existing seeds.
- [x] Cookbook examples: the review verified all 57 parse today. Add the
      cookbook lines to the same `help_reference_synopses_parse` test (same
      extraction, from the COOKBOOK section; skip lines containing `|` pipes
      and multi-line jq strings — document why).

### F-6 + README touch-ups

- [x] README.md:177: change "Every write command supports `--dry-run`" to
      "Write commands that modify existing files support `--dry-run`"
      (new/views set/types set write-or-refuse without one).
- [x] README.md pi-integration section (~:216-220): the `@v0.21.0` tag
      references are forward-dated for a tag that doesn't exist yet. Either
      gate the wording on release or add "(available once v0.21.0 is
      tagged)" — owner call; smallest honest fix is the parenthetical.

### Wrap-up

- [x] `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings &&
      cargo test --workspace -q` all green.
- [x] Dogfood: run `target/release/hyalo --help` and spot-check the three
      corrected synopses; run the UTF-8 repro from the review (36-byte file
      with `\xff\xfe` body) and confirm the new message.
- [x] Update [[reviews/deep-review-2026-08-27]] status to `resolved` and add
      a `related` link back to this iteration.

## Acceptance criteria

- [x] `hyalo summary --limit 5` still errors (unchanged), but `--help` no
      longer shows `[--limit N]` for summary.
- [x] Every `hyalo ...` line in COMMAND REFERENCE parses when instantiated
      (enforced by the new test, not just by hand).
- [x] `hyalo changelog add --category Added --message "x" --dry-run` works
      AND the help synopsis shows this flag form.
- [x] The UTF-8 repro file yields the new invalid-UTF-8 placeholder.
- [x] Hint execution gate covers changelog/okf/madr seeds.
- [x] README dry-run sentence no longer overclaims.
- [x] All quality gates green; no clippy/fmt/test regressions.
