---
type: iteration
title: "Iteration 258 — zero-result title~= hint toward body search"
date: 2026-08-31
status: in-progress
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

### HINT-1: zero-result `--property 'title~=...'` hint [0/1]

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

## Links

- [[iterations/iteration-256-envelope-help-forwarding-and-index-cost]]
