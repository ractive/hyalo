---
type: iteration
title: "Iteration 258 — zero-result title~= hint toward body search"
date: 2026-08-31
status: completed
tags:
  - iteration
  - dogfood-fixes
  - ux
branch: iter-258/zero-result-title-regex-hint
depends-on: "[[iterations/iteration-256-envelope-help-forwarding-and-index-cost]]"
---

# Iteration 258 — zero-result `title~=` hint toward body search

## Goal

Carry-over from [[iterations/iteration-256-envelope-help-forwarding-and-index-cost]]
(PR #298)'s dogfooding notes: `hyalo find --property 'title~=/DEC-25/'`
returns 0 results against a decision log whose `DEC-NNN` headings are `##`
body headings, not frontmatter titles — correct behavior, but the natural
query for "which DEC numbers are taken" is `hyalo find 'DEC-256'` (body
search) or a body regex, and nothing on the zero-result path currently says
so.

This is a UX polish item, not a bug: the LOW end of what 256 flagged. Weigh
it against `feedback_no_cli_surface_growth` (no new flags from dogfood
pressure) — the existing zero-result-hint machinery (did-you-mean over real
property values, same query with its most selective filter dropped, per
`rule-knowledgebase.md`) is the right home for this if it fits; if it
doesn't fit cleanly, close as won't-fix rather than growing a special case.

## Tasks

### HINT-1: zero-result `--property 'title~=...'` hint [1/1]

- [x] Read how the existing zero-result hint logic works (did-you-mean over
      property values, filter-dropping suggestion) and decide whether a
      `title~=` query that matches nothing can cheaply also check whether
      the same regex matches body text anywhere in the vault, and if so,
      surface `hyalo find '<pattern>'` (or `-e '<pattern>'` for a real
      regex) as a suggested next step. If the cost or complexity of that
      check doesn't fit the existing hint budget (it runs on every
      zero-result query, so it must stay cheap), close as won't-fix with a
      DEC explaining why, rather than adding a special-cased flag or slow
      path.

## Acceptance criteria

- [x] HINT-1 is either implemented (with a test covering the new hint
      trigger and its absence when body search also finds nothing) or
      explicitly closed as won't-fix with a DEC recorded.
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, all `xtask check-*`, `hyalo lint --strict`.

## Non-goals

- Any new CLI flag. This is a hint-text change at most — see
  `feedback_no_cli_surface_growth`.

## Outcome

Implemented, not closed as won't-fix — it fitted the existing machinery. A
zero-result `find` that carried a `--property K~=RE` filter now probes whether
the same regex matches body prose and, only when it does, leads the zero-result
hints with the equivalent `hyalo find -e '<RE>'`. See
[[decision-log#DEC-263: a zero-result property-regex query probes the body before hinting (2026-08-31)]]
for why the probe confirms the match instead of offering unconditional advice,
and for the budget that keeps it affordable.

- The probe is bounded at 512 files / 8 MiB with a first-match early exit, runs
  only on the zero-result path, only when a property regex filter is active, and
  never when the query already searched bodies (`PATTERN` / `-e`).
- Measured on this vault (437 files, 4 MB, release build): the worst case — a
  regex matching nothing anywhere, so the whole vault is probed — added ~10 ms
  to a ~20 ms query. A match short-circuits far earlier.
- No new flag and no new config key, per the non-goal below.
- Verified live: `hyalo find --property 'title~=/DEC-25/'` against this vault,
  whose `DEC-NNN` ids are `##` headings, now emits
  `-> hyalo find -e DEC-25  # No \`title\` matches that regex, but body text does — search bodies instead`.

Touched: `crates/hyalo-cli/src/commands/find/run.rs` (probe),
`crates/hyalo-cli/src/hints.rs` + `hints/zero_result.rs` (hint),
`dispatch.rs`/`run.rs` (plumbing), `templates/rule-knowledgebase.md`,
`.claude/skills/hyalo/SKILL.md`, `CHANGELOG.md`, `decision-log.md`.

## Links

- [[iterations/iteration-256-envelope-help-forwarding-and-index-cost]]
