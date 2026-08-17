---
title: Iteration 192 — CLI surface truth (help, hints, envelope, verbs)
type: iteration
date: 2026-08-06
status: completed
branch: iter-192/cli-surface-truth
tags:
  - iteration
  - cli
  - docs
  - hints
  - dx
related:
  - "[[reviews/codebase-review-2026-08-06]]"
  - "[[dogfood-results/dogfood-v0200-opus5-review-round]]"
  - "[[iterations/iteration-180-hint-trust]]"
---

# Iteration 192 — CLI surface truth

## Goal

Make the CLI's self-description true. Round 2 of the 2026-08-06 review found
the help text contradicting itself within a single `--help` output, four
mutually inconsistent enumerations of the same command set, one hint that
does not run, and one command that breaks the JSON envelope contract the
help promises.

The theme is **generate, don't restate**: every finding here is a
hand-maintained second copy of something the code already knows. Fixing the
copies without removing the duplication just resets the clock.

**Do NOT release; release is a separate user-gated step.**

## Context

Verified by running the binary at `c42fa6f`, not by reading strings:

- Commands that actually emit `total`: `find`, `tags summary`,
  `properties summary`, `backlinks`, `lint`, `views list`, `types list`.
  Four different places in the help claim four different subsets; a fifth
  list lives in the `--count` runtime error. None are correct.
- `hyalo tags --limit 0` (a hint the tool emits) errors.
  `hyalo tags summary --limit 0` works.
- `hyalo config --jq '.dir'` prints the whole object, unfiltered, no error.
- 28 of 29 harvested hints execute cleanly. Hint quality is good; the gate
  is what's missing.

iter-180 established hint trust but gated it with substring assertions
(`hints.rs:4029`: `.contains("--limit 0")`), which the broken hint satisfies.
That is the specific gap to close.

## Tasks

- [x] Introduce one shared constant (e.g. `LIST_COMMANDS` in `output.rs` or
      `cli/args.rs`) naming the commands that emit `total`, and derive from
      it: the OUTPUT paragraph, the `--count` flag help, the "Default output
      limits" block, the OUTPUT SHAPES note, and the `--count` runtime error
      message. Five call sites, one source.
- [x] Verify `views list` and `types list` belong in that constant — both
      currently return a `total` and accept `--count` (7 and 6), while the
      error message claims they do not.
- [x] Add the 8 missing commands to COMMAND REFERENCE: `changelog`, `config`,
      `lint`, `lint-rules`, `madr`, `new`, `okf`, `types`. Add an xtask gate
      asserting every clap subcommand appears in the reference, so the next
      command added cannot silently skip it.
- [x] Fix the global-flags reference block: `--format json|text|github`, the
      TTY-dependent default (not "default: json"), and add `--index-file`.
- [x] Fix `hints.rs:1439` — `["tags", "summary", "--limit", "0"]`.
- [x] Add the missing "show all N properties" hint to `properties summary`,
      mirroring the tags one, so the symmetric commands behave symmetrically.
- [x] Replace the substring-based hint assertions with an **execution-based**
      gate: harvest every hint the CLI emits across a fixture vault, run each,
      assert exit 0. New xtask target or e2e test — the review's 29-hint sweep
      is the model.
- [x] Wrap `hyalo config` in the standard envelope (`results` / `hints`),
      renaming the config setting to avoid the `hints` bool-vs-array
      collision, and make `--jq` work on it. If wrapping is judged a breaking
      change, at minimum make `--jq` error explicitly rather than silently
      ignoring the filter.
- [x] Add `list` as an alias for `summary` on `properties`/`tags`, and
      `summary` as an alias for `list` on `types`/`views`/`lint-rules`, so
      the verb a user learned in one group works in the others.
- [x] Document the positional form for `read` and `backlinks` in COMMAND
      REFERENCE and the COOKBOOK — it is what every hint emits and what the
      project's own agent rules prefer, but the reference shows only
      `-f/--file`.
- [x] Decide `mv`'s dual default (single-file writes immediately; batch
      defaults to dry-run). Recommend keeping the behaviour but making the
      asymmetry loud: reject `--apply` in single-file mode with "single-file
      mv applies by default; use --dry-run to preview" instead of silently
      accepting it as a no-op. Record as a DEC entry.

## Acceptance criteria

- [x] `LIST_COMMANDS` (or equivalent) is the single source for all five
      enumerations — `grep` finds no second hand-written list
- [x] xtask gate fails when a clap subcommand is missing from COMMAND
      REFERENCE — proven by temporarily removing one entry
- [x] execution-based hint gate runs every emitted hint and fails on a
      non-zero exit — proven by temporarily reverting the `hints.rs:1439` fix
- [x] `hyalo tags summary` hint runs clean; `hyalo properties summary` emits
      an equivalent working hint
- [x] `hyalo config --format json` carries `results` and an array `hints`;
      `hyalo config --jq '.results.dir'` prints the dir
- [x] `hyalo tags list`, `hyalo types summary`, `hyalo views summary`,
      `hyalo lint-rules summary` all exit 0
- [x] `hyalo mv --file a.md --to b.md --apply` errors with the guidance
      message — test name `mv_single_file_rejects_apply`
- [x] no phrase in `stale-help-patterns.toml` reappears; `cargo fmt`,
      `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean
