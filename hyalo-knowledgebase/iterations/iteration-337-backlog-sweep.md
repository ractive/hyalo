---
type: iteration
title: "Iteration 337: backlog sweep (jev timeout, MD007 abort, bookkeeping, three closed backlog items)"
date: 2026-11-08
status: in-progress
tags:
  - iteration
  - backlog
  - lint
branch: iter-337/backlog-sweep
related:
  - "[[backlog/md007-aborts-lint-on-multibyte-list-text]]"
  - "[[backlog/link-resolution-remaining-ambiguous-and-case-mismatch-links]]"
  - "[[backlog/link-resolution-block-reference-and-slug-anchors]]"
  - "[[backlog/wikilink-target-does-not-preserve-the-authored-md-suffix]]"
---

# Iteration 337: backlog sweep

Small items left over after iterations 330 to 336, none worth an iteration of its own.

## The MD007 abort (DEC-412)

Upstream MD007 (`mdbook-lint-rulesets` 0.16.2, the latest release) counts a leading tab as four columns and then cuts
the line at that count as a byte offset (`&line[indent..]`, `md007.rs` line 268). A tab-indented bullet whose text
starts with a multibyte character panics (exit 134 in a release build, `panic = "abort"`), taking every other note's
findings with it; with ASCII text the fix silently drops the item's first characters. `hyalo-mdlint` now gives MD007
a view of the note (`md007_view`, the sibling of `md034_view`) in which the leading whitespace of every tab-indented
unordered list item is written as spaces totalling the columns upstream counts there (a space one, a tab four); the rest
of the line, every other line and every terminator are verbatim. Upstream's list stack sees the indent it computes on
the original and its slice is in bounds. MD007 always gets the AST of the original note, and its findings on a
rewritten line are dropped (their fix range comes from the longer view line); MD010 still reports the tab.

The invariant, stated and tested: on every line except the tab-indented bullets, MD007's findings and fixes equal
upstream's on the original note. The first version blanked the bullet instead and claimed the same; the PR #419 review
showed it false (the children of a blanked bullet were measured one level too shallow, and a lone MD007 parsed the
blanked view, so findings differed alone and beside other rules). Alternatives and the correction are in DEC-412.

Mutation checks, all caught: with the view disabled the unit tests for the multibyte panic and the ASCII fix
`"  eep every character"` fail, and both original end-to-end tests fail with exit 101 (a panic under the unwinding test
profile, 134 in a release build); with a lone MD007 parsing its view instead of the original, the alone-versus-beside
test fails; with the bullet blanked again, the children test and the invariant test fail. Obsidian Hub, a copy of
checkout `a56b648b` with `[lint.rules.MD007] enabled = true`, plain `lint`: exit 134 and no output before, exit 1 with
all 6 520 notes checked (22 errors, 27 654 warnings) after; MD007 alone: exit 0, 3 457 warnings over 71 files, the same
3 457 findings as the blanking version.

## Three backlog items closed

- [[backlog/link-resolution-remaining-ambiguous-and-case-mismatch-links]]: `wont-do`, reported correctly, no tooling gap.
- [[backlog/link-resolution-block-reference-and-slug-anchors]]: `wont-do`, DEC-410 amends DEC-299.
- [[backlog/wikilink-target-does-not-preserve-the-authored-md-suffix]]: `wont-do`, DEC-411 amends DEC-310.

## Tasks

- [x] jev-helper: give the two slow `prepare.test.ts` tests a 30 s per-test timeout and the transport smoke a 30 s `spawnSync` timeout
- [x] Iterations 330 and 336: tick the PR tasks (PR #412 and PR #418 are merged; `quality-gates`, which runs check-help-drift, passed on #418) and set `status: completed`
- [x] Docs: the staged temp files are named `.hyalo-tmp-*`, not `.tmpXXXXXX` (`.claude/CLAUDE.md`, `rule-knowledgebase.md`), said once
- [x] MD007: `md007_view` guard in `hyalo-mdlint`, unit tests, end-to-end tests, mutation check, DEC-412, backlog item completed
- [x] Close the ambiguous and case-mismatch backlog item as `wont-do` with its Outcome (figures re-measured on the Hub: 100 ambiguous, 48 case mismatches)
- [x] Close the block-reference backlog item as `wont-do` with its Outcome, DEC-410 amending DEC-299
- [x] Close the authored-`.md` backlog item as `wont-do`, DEC-411 amending DEC-310; state the rule in `.claude/CLAUDE.md`, the rule template and the skill template
- [x] CHANGELOG entries for the user-visible fixes
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -q`
- [x] jev `bun install --frozen-lockfile`, `bun run typecheck`, `bun run build`, `bun test`
- [x] xtask gates that cover the touched files (`check-help-drift`, `check-pi-package-sync`, `check-jq-recipes`, plus `check-bundled-skills`, `check-codex-package`, `check-command-reference`, `check-feature-fanout`, `check-jev-assets`)
- [x] `hyalo lint` on the knowledgebase: 0 errors, no new warnings from the touched files
- [x] Open the PR

## Results

- jev: `bun test` 26 pass, 0 fail (2.27 s) after the change.
- `cargo test --workspace -q`: 6 034 passed, 0 failed (after the PR #419 review round).
- `check-help-drift` passes locally (about 10 minutes with the debug binary), run as the built `xtask` binary with an absolute `CARGO_MANIFEST_DIR`.
