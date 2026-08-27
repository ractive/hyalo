---
type: iteration
title: "Iteration 242 — remove the --iteration natural-key flag"
date: 2026-08-27
status: completed
tags: [iteration]
branch: iter-242/remove-iteration-flag
---

# Iteration 242 — remove the `--iteration` natural-key flag

## Goal

Remove the `--iteration <ID>` flag (iter-235, widened in iter-238 and
iter-241) from every command and delete its implementation, per the owner's
"featureitis" verdict: a third addressing mechanism next to `--file`/`--glob`
whose padding/suffix/recursion rules are harder to predict than the glob it
replaced. The replacement is documentation — help text and skills now teach
globbing sequence-keyed files directly (`--glob '**/iteration-02-*.md'`).

## Tasks

- [x] Remove the `--iteration` flag from `find`, `set`, `read`, `task`,
      `backlinks` (args.rs, inputs.rs, conflict lists, help text, examples)
- [x] Delete `commands/iteration.rs` (glob resolution + single-file
      selection rewrite) and its dispatch/call-site wiring
- [x] Delete `hyalo-core::iteration_id` and the now-unused
      `FilenameTemplate::{has_n_placeholder, to_glob_for_id, n_pad_width,
      to_glob_variants_for_id}`
- [x] Replace the teaching: find long help gets a SEQUENCE-KEYED FILES
      section, the bundled skills (templates, pi-package, .claude) get a
      glob-addressing note
- [x] Drop the `--iteration` tests; keep the `--filenames-only`/`--filenames0`
      tests that used the flag as a filter by switching them to `--glob`
- [x] CHANGELOG [Unreleased] > Removed entry; DEC-242 in [[decision-log]]
- [x] `cargo fmt` / `clippy -D warnings` / `cargo test --workspace` clean;
      all four xtask gates green

## Acceptance criteria

- [x] `hyalo find --iteration 206` fails with clap's "unexpected argument"
      error on every command that had the flag
- [x] The documented replacement works: `hyalo find --glob '**/iteration-02-*.md'
      --filenames-only` resolves the file, then `hyalo read --file <path>` (or a
      positional path) reads it
- [x] No `--iteration` string remains in help text, README, COMMAND
      REFERENCE, skills, or the pi package

## Non-goals

- Removing the `{n}` filename_template feature itself (used by
  `new`/`lint`/type inference) — only the addressing flag goes
- `--filenames-only`/`--filenames0`, which shipped alongside it
