---
title: Iteration 213 — config & UX polish batch
type: iteration
date: 2026-08-23
status: completed
branch: iter-213/config-ux-polish
tags:
  - iteration
  - config
  - ux
related:
  - "[[dogfood-results/dogfood-v0210-pre2-integrity-wave]]"
  - "[[iterations/iteration-201-config-trust]]"
---

# Iteration 213 — config & UX polish batch

## Goal

Clear the config-discovery and UX findings from the 2026-08-23 dogfood:
the silent subdirectory-config miss (the un-fixed half of iter-201's bug
class), a machine-readable malformed-config signal, the BUG-14 cluster
of contract inconsistencies, and the small message/formatting nits.

## Context

From [[dogfood-results/dogfood-v0210-pre2-integrity-wave]], UX-1/2/3/5
and BUG-14:

- **UX-1**: `.hyalo.toml` is read only from cwd. Running from inside the
  vault (`cd hyalo-knowledgebase && hyalo lint`) silently re-roots on
  built-in defaults with no diagnostic — while the `--dir` path now
  warns loudly. The most common config accident gets no signal.
- **UX-2**: on a malformed config, `hyalo config` exits 0 with populated
  defaults and no `malformed` field; the diagnostic is stderr-only.
  Also `raw_contents` (a multi-KB single-line blob) dominates the JSON.
- **BUG-14**: invalid `[changelog] path` config refusals exit 2 with
  single-line wording (runtime refusals use exit 1 + two-path form);
  `views run` rejects positional BM25 patterns despite help claiming
  `find --view` equivalence, and its `-e` help references a PATTERN
  argument that subcommand does not have; `config_excluded` counts
  excluded *titles* not suppressed *candidates* (excluding one title
  that removes 130 candidates reports `1`); `create-index -o
  /tmp/my-index` is a verbatim help EXAMPLE that fails the boundary
  check, and the `--index-file` help ("absolute paths are used as-is")
  contradicts the guard.
- **UX-3**: the index-mismatch warning prints the identical vault path
  twice and leaks `Some("en-us")` Rust formatting; the actual difference
  (site prefix) is buried.
- **UX-5**: fatal single-file parse errors print `warning:` while
  exiting 1; `--fix` can report a rule as both fixed and conflicted
  (display only); `tags --limit` lacks the "showing N of M" footer
  `properties --limit` has; on the `--dir` escape hatch the stale
  malformed-config warning prints before the "does not apply" note;
  hint `--format` flag placement is inconsistent between read-only and
  writes hints.

## Tasks [10/10]

- [x] UX-1: when no cwd config exists, walk ancestors for `.hyalo.toml`
      whose configured vault contains cwd; either adopt it (preferred —
      record as a DEC, note the behavior change) or emit a loud
      stderr warning naming the found config and the `--dir`/`cd`
      remedies. Either way, the silent case must be impossible.
- [x] UX-2: add `malformed: true` + the parse error to `hyalo config`
      output (text and JSON) when the config failed to parse; move
      `raw_contents` behind `--raw` (or similar opt-in).
- [x] BUG-14: route `[changelog] path` config-level refusals through the
      shared boundary-refusal formatter (exit 1, two-path wording).
- [x] BUG-14: give `views run` a positional PATTERN with the same
      semantics as `find` (BM25, mutually exclusive with `-e`), making
      the help claim true; fix the `-e` help text.
- [x] BUG-14: make `config_excluded` count suppressed candidates, or
      rename to `config_excluded_titles` and add the candidate count —
      the stated purpose ("a bare run stays explainable") must hold.
- [x] BUG-14: fix the `create-index` help EXAMPLE (add
      `--allow-outside-vault` or use an in-vault path) and reword the
      `--index-file` help to state the boundary rule; document the
      read-only-corpus indexing workflow in the command help.
- [x] UX-3: reword the index-mismatch warning to state only the
      differing field, without `Some(…)` leakage.
- [x] UX-5 batch: `error:` prefix on fatal single-file parse failures;
      suppress the duplicate fixed/conflict display line; add the
      truncation footer to `tags --limit`; suppress the stale
      malformed-config warning once `--dir` has switched vaults; unify
      hint flag placement.
- [x] (from superseded iter-207a) Update `md047_fix`'s doc comment in
      `crates/hyalo-mdlint/src/engine.rs` (and
      [[docs/upstream-mdbook-lint-reports]] if any "not filed" language
      remains) to cross-reference the filed upstream issue
      joshrotenberg/mdbook-lint#495.
- [x] Docs/help/CHANGELOG sync for every behavior change above; extend
      the executed-hint and command-reference xtask gates where they
      apply.

## Acceptance criteria [6/6]

- [x] Running any command from a vault subdirectory either uses the
      ancestor config or prints a warning naming it — never silent
      defaults
- [x] A JSON consumer can detect a malformed config from `hyalo config`
      output alone
- [x] All boundary refusals (runtime and config-level) use exit 1 and
      the two-path form
- [x] `hyalo views run <view> <pattern>` returns the same results as
      `hyalo find <pattern> --view <view>`
- [x] The `create-index` help example runs verbatim successfully
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Frontmatter format preservation —
  [[iterations/iteration-214-frontmatter-format-preservation]].
- Unifying `.results` JSON shapes across commands — needs its own
  design pass.
