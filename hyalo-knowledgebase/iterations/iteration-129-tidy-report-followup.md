---
title: Iteration 129 — Tidy report follow-up
type: iteration
date: 2026-05-08
tags:
  - iteration
  - lint
  - links
  - schema
  - ux
status: in-progress
branch: iter-129/tidy-report-followup
---

## Goal

Dogfooding hyalo against a sibling project's knowledgebase (`wardrobe-assistants.ch`)
during a `/hyalo-tidy` pass surfaced six concrete friction points. The pass moved
33 files into a folder structure (iterations/, runbooks/, tool-reports/, legal/,
notes/) and produced a tool report capturing what worked and what didn't. The
report lives outside this repo (in the sibling project's `kb/tool-reports/`),
but the findings reproduce against any non-trivial vault and are worth fixing
before the next tidy pass.

This iteration tightens link correctness (`links fix` overcorrection, `../`
false positives), gives schema validation a strict mode, and fixes the CLI's
`--format json`-by-default ergonomics that force every LLM-driven invocation
to append `--format text`. Two findings (rewriting backticked path-shaped
tokens during `mv`, and a per-file ignore directive for untyped files) are
**deferred** — see [[#Deferred]].

## Approach

Three coherent buckets:

1. **Link correctness.** `hyalo links fix` should never rewrite a markdown link
   that already resolves to the right file via its source-relative directory.
   Today it treats bare-basename markdown links as vault-relative
   ([`crates/hyalo-core/src/link_fix.rs:285-296`]) and gets confused after a
   `mv` shuffles files into a subfolder. Separately, link resolution already
   normalizes `..` correctly via `normalize_target` →
   `normalize_path_components` ([`crates/hyalo-core/src/link_graph.rs:486-521`]),
   so the `../`-flagged-as-unresolved finding is most likely surfacing from a
   *different* code path (probably the lint-side resolver) — needs investigation
   before patching.

2. **Schema strictness.** Today `lint.rs` emits `Severity::Warn` for "no type
   property" and for undeclared frontmatter fields
   ([`crates/hyalo-cli/src/commands/lint.rs:760-845`]). Add a `strict` mode
   (config: `[lint] strict = true` in `.hyalo.toml`; CLI: `hyalo lint --strict`)
   that promotes those specific warnings to errors. Files that are
   intentionally untyped (snapshot artefacts, etc.) can be handled by
   declaring a permissive type for them in `.hyalo.toml` — no per-file
   ignore directive in this iteration.

3. **Output defaults.** Detect a TTY on stdout via `std::io::IsTerminal` (in
   stdlib since 1.70 — no new dep needed). When the user hasn't passed `--format`
   and stdout is a TTY and `.hyalo.toml` doesn't pin a default, default to
   `text`. When stdout is piped, keep `json` as the default so existing scripts
   don't break. This is wired through `Cli::format` in
   [`crates/hyalo-cli/src/cli/args.rs:131-137`] and the `Format` enum in
   [`crates/hyalo-cli/src/output.rs:29-49`]. Update help text accordingly.

## Deferred

- **Plain backtick prose references rewritten by `hyalo mv`.** Things like
  `` `kb/iac-runbook.md` `` aren't markdown or wiki links, so `mv` can't see
  them. Rewriting all backtick path-shaped tokens is risky (false positives in
  code blocks, inline code about *historical* paths, etc.). Open question:
  is the right behavior "frozen-history files keep stale references on
  purpose"? Defer until we have a second concrete case demanding the feature.
  Track in backlog if it comes up again.
- **Per-file ignore directive for `missing-type` warnings** (e.g. a `hyalo:
  ignore` frontmatter flag). Dogfooded use cases are too thin to justify
  another config surface — a feature added "just in case" is unlikely to
  pay for its own maintenance. If a strict pass keeps tripping on a real
  file, declare a permissive schema type for it in `.hyalo.toml` instead.

## Tasks

- [x] Investigate finding 1: write a failing unit test in
  `crates/hyalo-core/src/link_fix.rs` that builds two files in the same folder
  (`a/foo.md` linking to `bar.md`, with `a/bar.md` existing) and asserts no
  `case_mismatches` / `LinkCaseMismatch` `FixPlan` is produced after a
  `links fix` pass — pin down the exact code path that overcorrects
- [x] Fix finding 1: change the bare-basename branch in
  `detect_broken_links_from_index` (and the twin in `detect_broken_links`) so
  markdown links without `/` are resolved against the source file's directory
  before classification, matching the behavior of links that already contain a
  slash; preserve wikilink semantics (still vault-relative)
- [x] Investigate finding 2: write a failing unit/e2e test for `hyalo lint`
  against a file containing `[x](../sibling/y.md)` where `y.md` exists; locate
  whichever resolver is reporting "unresolved" (likely a lint rule path
  separate from `link_fix.rs`, since `normalize_target` already collapses `..`
  correctly)
- [x] Fix finding 2: route the offending lint check through the same
  `normalize_target` / `normalize_path_components` helper used by `link_fix`,
  so `../`-relative markdown links resolve against the file's own directory
- [x] Add `[lint] strict = false` to the config schema in `.hyalo.toml`
  parsing (default `false` for backwards compat); thread it into the
  `LintConfig` struct used by `commands/lint.rs`
- [x] Add `--strict` CLI flag to `hyalo lint` that overrides the config value
  for a single invocation
- [x] In `commands/lint.rs` around lines 760-845, when strict mode is on,
  promote `severity: Severity::Warn` to `Severity::Error` for the
  "no 'type' property" warning and for the "undeclared property in frontmatter"
  warning; leave other warnings (no tags, etc.) alone — strict is specifically
  about schema declaredness
- [x] Output-format default: change `Cli::format` resolution in `run.rs` /
  `output.rs` so that when `format` is `None` after merging CLI flag + config,
  use `std::io::IsTerminal` on `std::io::stdout()` to pick `Format::Text` for
  TTY and `Format::Json` for pipes. Existing explicit `--format` and
  `.hyalo.toml` defaults still win
- [x] Update `--format` help text in `crates/hyalo-cli/src/cli/args.rs:134-137`
  to reflect the new TTY-aware default
- [x] Update README and `.claude/CLAUDE.md` to mention: `text` is auto-default
  for interactive use, `--format text` no longer needs to be appended
  manually, and `--strict` for schema enforcement
- [x] Update the hyalo skill (`.claude/skills/hyalo/SKILL.md` and the global
  copy under `~/.claude/skills/hyalo/`) to drop the "always append
  `--format text`" advice — it becomes unnecessary noise once piped vs TTY is
  detected automatically. Replace with a brief note about strict mode being
  available
- [x] Update the `hyalo-tidy` skill (`.claude/skills/hyalo-tidy/SKILL.md`
  and the global copy under `~/.claude/skills/hyalo-tidy/` if present) to
  run `hyalo lint --strict` as part of the tidy workflow, so a tidy pass
  fails fast on missing-type / undeclared-property issues instead of
  silently leaving them as warnings
- [x] Unit tests: bare-basename intra-folder link is not flagged after `mv`;
  `../sibling.md` resolves correctly; `[lint] strict = true` promotes the two
  targeted warnings to errors; `IsTerminal`-based default selection picks
  `Json` when stdout is piped (use a fake/redirected handle in the test)
- [x] e2e tests: `hyalo links fix` after a `hyalo mv` does not rewrite
  in-folder bare-basename links (regression test for the original tidy pass);
  `hyalo lint --strict` exits non-zero on a file with no `type` property and
  zero on a clean vault; `hyalo find ...` (no `--format`) prints text when
  run interactively (test via piping a TTY allocator if available, otherwise
  assert behavior with `IsTerminal` mocked)
- [x] Dogfood: `cargo build --release`; re-run a tidy pass against a fresh copy
  of the wardrobe-assistants.ch knowledgebase (or any sibling vault), confirm
  the seven manual reverts from the original report are no longer needed and
  that `hyalo lint` reports zero false-positive `../` unresolved warnings.
  Capture observations in a fresh tool report
- [x] Run `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace -q` and fix every issue before opening the PR

## Acceptance Criteria

- [x] `hyalo links fix` after `hyalo mv` leaves bare-basename markdown links
  unchanged when the target exists in the source file's own directory
- [x] `hyalo lint` does not report `[x](../sibling/y.md)` as unresolved when
  `sibling/y.md` exists relative to the file's directory
- [x] `hyalo lint --strict` (or `[lint] strict = true` in `.hyalo.toml`) exits
  non-zero when any file is missing a `type` property or carries undeclared
  frontmatter fields, and exits zero on a clean vault
- [x] Running `hyalo find ...` (no `--format`) interactively in a terminal
  prints text by default; the same command piped to a file produces JSON
- [x] `--format` and `.hyalo.toml` `format` settings still take precedence over
  the TTY-detected default when explicitly set
- [x] Help text, README, `.claude/CLAUDE.md`, and the hyalo skill all reflect
  the new defaults; no leftover guidance saying "always append `--format text`"
- [x] The `hyalo-tidy` skill invokes `hyalo lint --strict` as part of its
  workflow, so a tidy pass surfaces schema gaps as errors
- [x] Backtick-rewriting (finding 3) and per-file ignore directive
  (finding 5) are documented as deferred in the iteration notes; no code
  change is shipped for either in this iteration
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo test --workspace -q` are all clean
