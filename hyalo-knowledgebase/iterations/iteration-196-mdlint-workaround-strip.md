---
title: Iteration 196 — strip mdbook-lint convert_fix workarounds (upstream-gated)
type: iteration
date: 2026-08-17
status: completed
branch: iter-196/mdlint-workaround-strip
tags:
  - iteration
  - dependencies
  - upstream
related:
  - "[[docs/upstream-mdbook-lint-reports]]"
  - "[[iterations/iteration-193-vault-side-effects-and-dep-diet]]"
---

# Iteration 196 — strip mdbook-lint convert_fix workarounds (upstream-gated)

## Goal

Remove the downstream workarounds in `crates/hyalo-mdlint/src/engine.rs`
(`convert_fix`, roughly lines 694-890) that exist only to compensate for
upstream mdbook-lint autofix-coordinate bugs, once upstream ships them
fixed. Reserved as its own iteration by iter-193's non-goals ("Do not
attempt to fix upstream #456 here").

## Trigger — do NOT start until this holds

A published `mdbook-lint-core` / `mdbook-lint-rulesets` release **newer
than 0.15.2** whose history contains upstream PRs
[#486](https://github.com/joshrotenberg/mdbook-lint/pull/486) (MD011/
MD034/MD047 corruption fixes, merged 2026-08-05),
[#492](https://github.com/joshrotenberg/mdbook-lint/pull/492) (MD018
continuation-line fix — closes our #491, merged 2026-08-18), and
[#493](https://github.com/joshrotenberg/mdbook-lint/pull/493) (the #456
coordinate contract: 1-based Unicode-scalar `Position`, half-open `Fix`,
checked byte-conversion APIs, explicit LF/CRLF/EOF semantics — closes
issue 456, merged 2026-08-18). **As of 2026-08-18 all three are on upstream
`main` but NO release contains them — latest is still 0.15.2.** Check
with `cargo info mdbook-lint-core` / their releases page. The pending
release is visible as their open release-plz PR
[#484](https://github.com/joshrotenberg/mdbook-lint/pull/484)
("chore: release v0.16.0") — its cargo-semver-checks output already
flags `mdbook-lint-core` as API BREAKING (plus a new
`Config.experimental_rules` field from their #480). When #484 merges and
v0.16.0 hits crates.io, this iteration is runnable. Read #493's
migration notes before bumping.

## Context

- The workarounds and their upstream counterparts are documented, with
  code excerpts, in [[docs/upstream-mdbook-lint-reports]] §1 and in the
  posted comment on upstream #456
  (issuecomment-5319878913). Summary of what #486 fixed on their `main`:
  MD011 inclusive end column, MD034 Liquid-swallowing + char/byte length
  mix, MD047 no-op range.
- Upstream #493 (merged 2026-08-18) implements the "contract half" of
  issue 456: unit-defined coordinates, half-open ranges, checked
  position-to-byte conversion APIs, and explicit newline/EOF constructors
  replacing the replacement-driven heuristic. If the triggering release
  contains it, the per-rule `rule_uses_byte_columns` allowlist
  (`engine.rs:702`) and the `line_len + 1` replace-vs-insert heuristic
  become candidates for FULL removal in favor of the new APIs — a bigger
  win than the original scope. Note #493 is an API redesign: expect
  compile-visible changes in `convert_fix`, and treat the bump as a
  migration, not a drop-in.
- The MD011 guard was deliberately written to self-neutralize
  (`content[end..].starts_with(']')`), so it is *safe* under a fixed
  upstream — this iteration removes it for clarity, not correctness.
- iter-193's audit notes two RUSTSEC ignores (`bincode` 1.x, `yaml-rust`)
  arriving via `comrak -> syntect`, and a `toml` 0.5 duplicate tracked as
  upstream #459 — both worth re-checking on any bump.

## Tasks

- [x] Bump `mdbook-lint-core` / `mdbook-lint-rulesets` in
      `crates/hyalo-mdlint/Cargo.toml` to the triggering release. Read the
      release notes for anything beyond #486 (rule-behaviour changes can
      shift KB lint counts); diff `md018.rs` and any rule in the
      `rule_uses_byte_columns` allowlist for silent column-unit changes,
      as done in iter-193's audit.
- [x] Remove the MD011 `end += 1` guard, the MD034 Liquid pull-back, and
      the MD047 recompute path from `convert_fix`. For each, first write
      (or re-enable) a fixture test that fails under 0.15.2 semantics and
      passes under the new release — proving the upstream fix is actually
      in the shipped crate, not just on their `main`.
- [x] MD018 (#491, fixed upstream by #492): add a regression fixture
      (continuation line `#472` not flagged; standalone `#foo` still
      flagged; mid-line `PR #472` not flagged) and delete the latent-bug
      note in iter-193's Part B audit trail.
- [x] #493 adoption: migrate `convert_fix` to the new `Position`/`Fix`
      contract and checked conversion APIs. If the contract holds as
      documented, DELETE `rule_uses_byte_columns` and the `line_len + 1`
      heuristic entirely (each behind its own fixture proving the shipped
      crate honors the contract for a rule that previously needed the
      workaround, incl. a multibyte + CRLF case). If any rule still
      violates the contract in the shipped release, keep the minimal
      guard, file it upstream, and record the exception here.
- [x] Re-check `deny.toml`: do the two RUSTSEC ignores still resolve
      through `comrak -> syntect`? Does `toml` 0.5 dedupe (upstream #459)?
      Record before/after unique-crate count if the tree changed.
- [x] Run the full KB + fixture lint corpus before/after and diff violation
      counts; investigate any delta beyond the expected MD011/MD034/MD047
      fix-output changes.

## Deferred — needs the user (not a checkbox: cannot be done unattended)

Third-party repo writes are user-gated, and this iteration ran unattended, so
the following is carried forward rather than left as a permanently unticked
task. See [[iterations/iteration-194-post-upstream-mdbook-lint-reports]] for
the pattern.

- Post a short follow-up comment on upstream
  [#456](https://github.com/joshrotenberg/mdbook-lint/issues/456) reporting
  the embedder result: the 0.16.0 contract let hyalo delete the byte-column
  allowlist, the hand-rolled coordinate walk, and the MD011/MD034 guards
  outright.
- File the MD047 CRLF gap described under "The one surviving exception"
  below (hard-coded `"\n"` insertion, and `check_file_ending` under-counting
  CRLF terminators).

## Acceptance criteria

- [x] Each removed workaround has a fixture test proving the upstream fix
      is present in the *published* crate hyalo now pins
- [x] `convert_fix` contains no compensation for MD011/MD034/MD047 range
      bugs; the byte-column allowlist and `line_len + 1` heuristic are
      removed under the #493 contract (or every survivor is justified by
      a named, upstream-filed contract violation in the shipped release)
- [x] `cargo deny check` clean; RUSTSEC ignore list re-verified and
      annotated with the re-check date
- [x] KB lint counts unchanged except deltas explained by the release notes
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Outcome (2026-08-22)

**Trigger satisfied.** `mdbook-lint-core` / `mdbook-lint-rulesets` **0.16.0**
were published to crates.io on 2026-08-20 (upstream release PR #484 merged
2026-08-20 17:54Z). The release notes name all three gating PRs: #486
(MD011/MD034/MD047 autofix corruption), #492 (MD018 paragraph continuation
lines), #493 (exact autofix coordinates).

### What was deleted

`crates/hyalo-mdlint/src/engine.rs` lost ~200 lines of compensation:

- `rule_uses_byte_columns` — the per-rule byte-vs-char allowlist. 0.16.0
  defines `Position` columns as 1-based Unicode scalars for **every** rule.
- `line_col_to_byte` — the hand-rolled walk. Replaced by upstream's checked
  `Position::to_byte_offset`, which also rejects offsets inside a CRLF pair.
- The MD011 `end += 1` inclusive-end guard — `Fix` ranges are half-open now.
- `trim_md034_liquid` / `Md034Trim` — upstream's URL boundary scan stops
  before `{%` and `{{` on its own (`md034.rs:228-230`).
- The `line_len + 1` replace-vs-insert heuristic — MD009 and friends now use
  `Fix::line_replacement` with the document's own line ending, so
  insertion-shaped and replacement-shaped fixes are distinguishable from the
  range alone.

`convert_fix` is now nine lines: two checked conversions, a `start <= end`
sanity check, and the `DiagFix`.

`crates/hyalo-mdlint/src/rules/hyalo001.rs` was migrated too — it emitted
byte columns, which the old allowlist papered over; it now uses
`Position::from_byte_offset_in_line` / `Position::line_end`.

### The one surviving exception

`md047_fix` is kept, but **only for bodies containing CRLF**; LF bodies take
upstream's fix unchanged. Shipped 0.16.0
`mdbook-lint-rulesets/src/standard/md047.rs` still:

1. hard-codes `Fix::insertion(…, "\n", …)` for the missing-EOF-newline
   branch, which would append a bare LF to a CRLF file; and
2. counts trailing terminators with
   `content.chars().rev().take_while(|&c| c == '\n')`, which stops at the
   `\r` of the preceding CRLF — so MD047 never fires on a CRLF file with
   extra trailing blank lines (a detection gap upstream owns).

Recorded in [[docs/upstream-mdbook-lint-reports]]; **not filed upstream** —
third-party repo writes stay user-gated and this iteration ran unattended.
That, plus the follow-up comment on upstream #456, is the outstanding manual
step.

### Verification

- 74 unit tests in `hyalo-mdlint` (6 new iter-196 fixtures), 1454+999+68+22
  workspace tests green; `cargo fmt`, `clippy -D warnings`, all four xtask
  `check-*` gates clean.
- `cargo deny check`: **advisories ok, bans ok, licenses ok, sources ok**.
  Both RUSTSEC ignores (`bincode` RUSTSEC-2025-0141, `yaml-rust`
  RUSTSEC-2024-0320) still resolve through `comrak 0.21 -> syntect 5.3` and
  nothing else; annotated in `deny.toml` with the 2026-08-22 re-check date.
  `toml` 0.5 is **still** duplicated against `toml` 1.x (upstream #459 not
  fixed in 0.16.0). Unique crate count unchanged at **136** — the only
  `Cargo.lock` movement besides the two versions is `glob` shifting from
  `mdbook-lint-rulesets` to `mdbook-lint-core` (upstream #489).
- **Lint corpus diff, main vs branch — violation counts identical** on all
  three corpora: own KB (4 HYALO002), `~/devel/docs` (2249 across 10 rules),
  `~/devel/vscode-docs` (1231 across 11 rules).
- **Autofix output diff** on `vscode-docs`: 3 files differ, all the same
  class and all improvements — MD010 hard-tab fixes on lines containing
  multibyte characters (`✘`, `’`) that the old char-walk **dropped** are now
  applied. Post-fix residual: main 270 violations (1 unfixed MD010), branch
  269 (0). Re-running `--fix` on the branch output is a no-op, so the fix
  pass still converges.

## Non-goals

- Any release; release stays user-gated.
- Reworking hyalo-side lint features beyond the translation layer —
  this iteration is dependency migration + workaround deletion only.
