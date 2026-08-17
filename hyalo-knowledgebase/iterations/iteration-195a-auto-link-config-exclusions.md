---
title: Iteration 195a — persistent auto-link exclusions in .hyalo.toml
type: iteration
date: 2026-08-17
status: in-progress
branch: iter-195a/auto-link-config-exclusions
tags:
  - iteration
  - links
  - auto-link
  - config
related:
  - "[[backlog/done/auto-link-config-exclusions]]"
---

# Iteration 195a — persistent auto-link exclusions in .hyalo.toml

## Goal

Promote the last open backlog item to done: a `[links.auto]` section in
`.hyalo.toml` that persists the exclusions and `first_only` preference that
today must be retyped on every `hyalo links auto` invocation. Full problem
statement, external-user evidence (~94% noise on a title-heavy vault), and
prior-art table live in [[backlog/done/auto-link-config-exclusions]] — read it
first; this plan only adds the implementation shape.

**Do NOT release; release is a separate user-gated step.**

## Context

Verified at `56480cb`:

- `[links]` parsing: `LinksConfig` struct at
  `crates/hyalo-cli/src/config.rs:21`, parsed around `config.rs:412`;
  existing fields `case_insensitive`, `frontmatter_properties`. A nested
  `[links.auto]` table fits this structure.
- CLI flags to mirror: `exclude_title` (args.rs:1986), `first_only`
  (args.rs:1991), `exclude_target_glob` (args.rs:1994).
- Persistence precedent: `lint-rules set` writing `[lint]` — follow its
  config-shape conventions. Snake_case keys (`exclude_titles`,
  `exclude_target_globs`, `first_only`).
- CI version-skew gotcha (see memory/project notes 2026-07-19): a new
  config key is warned-and-ignored by the *released* hyalo that lint-kb CI
  installs. Do NOT add `[links.auto]` to this repo's own `.hyalo.toml`
  until after the next release.

## Tasks

- [ ] Extend `LinksConfig` with an optional `auto` table:
      `exclude_titles: Vec<String>`, `exclude_target_globs: Vec<String>`,
      `first_only: Option<bool>`. Unknown-key warning behaviour must match
      the rest of config parsing.
- [ ] Merge semantics in the `links auto` command path: config lists and
      CLI lists are UNIONED (flags extend, never replace); `--first-only`
      flag overrides config when given, config applies otherwise. Unit
      tests for all four first_only combinations and both list unions.
- [ ] Surface config-driven exclusions in the output: a `config_excluded`
      count (JSON envelope + text renderer) so a bare `links auto` run
      stays explainable. Zero is omitted, matching `links.out_of_vault`
      precedent.
- [ ] `hyalo config` shows effective `[links.auto]` settings (both
      formats; envelope under `results` per iter-192's DEC-064-adjacent
      contract).
- [ ] Decide and record (decision log entry): whether `links fix
      --ignore-target` gains an `--exclude-target-glob`-style alias for
      naming alignment, or is documented as-is. Alias must be
      non-breaking; default to document-only if in doubt.
- [ ] e2e: a fixture vault with `[links.auto]` config exercising
      config-only, config+flags union, and first_only override; hint
      execution and check-command-reference gates stay green.
- [ ] Docs in the same PR: `links auto --help`, COMMAND REFERENCE,
      configuration docs page, and the knowledgebase rule template if it
      mentions `links auto`. README only if its existing prose becomes
      wrong (README is not a feature list).
- [ ] Move [[backlog/done/auto-link-config-exclusions]] to `backlog/done/` with
      its ACs ticked (verify each against the landed behaviour first).

## Acceptance criteria

- [ ] `[links.auto] exclude_titles` suppresses matches with no CLI flags
- [ ] CLI `--exclude-title` extends (not replaces) the config list; same
      for `--exclude-target-glob`
- [ ] `first_only = true` in config behaves like the flag; explicit flag
      wins per run
- [ ] `hyalo config` reports the effective `[links.auto]` settings
- [ ] Bare `links auto` output shows `config_excluded` when config
      exclusions removed candidates
- [ ] Repo's own `.hyalo.toml` is NOT modified (CI version skew)
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- The stopword/common-English-word warning heuristic from the backlog's
  stretch section — file it as a new backlog item if still wanted after
  this lands.
- Persisting any other `links auto` flag (e.g. `--min-confidence` belongs
  to `links fix`); scope is exactly the three keys above.
