---
title: Iteration 200 — links apply-path integrity (release blocker)
type: iteration
date: 2026-08-18
status: planned
branch: iter-200/links-apply-integrity
tags:
  - iteration
  - links
  - data-integrity
related:
  - "[[dogfood-results/dogfood-v0210-pre-release-iters-191-198]]"
---

# Iteration 200 — links apply-path integrity (release blocker)

## Goal

Fix the two HIGH data-corruption bugs in the `links fix/auto --apply` paths
(dogfood H-1, H-2) plus the mislabeled cross-vault fallback that amplifies
them (M-1). This is the **v0.21.0 release blocker**: both bugs are reachable
through hyalo's own recommended hints, and on a real GitHub Docs copy plain
`links fix --apply` modified 1,097 files while making the broken count go
UP (6,565 → 6,582).

**Do NOT release; release is a separate user-gated step.**

## Context

All repros are written out in full in
[[dogfood-results/dogfood-v0210-pre-release-iters-191-198]] (H-1, H-2, M-1).
Code anchors, verified at `931a226`:

- H-1: the writer emits vault-root-relative targets while the resolver
  treats bare targets as file-relative. `link_fix.rs:867` already branches
  on `span.link.target.starts_with('/')` — the write side drops that
  leading `/` on rewrite. Resolution semantics live in
  `hyalo-core/src/link_resolve.rs` / `discovery.rs`; rewrite emission in
  `link_rewrite.rs` / `link_write.rs`.
- H-2: `hyalo-core/src/auto_link.rs` already excludes inline code and
  whole-text link matches; it does NOT exclude (a) markdown link
  destinations `(...)`, (b) bare URLs in prose, (c) substrings inside an
  existing link's label. Repro: page titled `net` +
  `[x](https://pkg.go.dev/x/actions.summerwind.net/v1)`.
- M-1: `LinkCaseMismatch` strategy in `hyalo-core/src/link_fix.rs` matches
  a basename anywhere in the vault at confidence 1.0 and lands in the
  default apply bucket — `[GitHub Actions](/actions)` rewritten to
  `graphql/reference/actions.md` even though `actions/index.md` exists.

## Tasks

- [ ] H-1: make site-absolute fixes round-trip. A fix for a link written
      `/a/b` must be emitted in a form that the resolver actually resolves
      — either preserve the leading `/` (site-absolute stays
      site-absolute) or emit a correctly file-relative path. Add unit
      tests for absolute→absolute and the minimal repro from the report
      (`/how-tos/old-home/moved-page` → target at
      `how-tos/new-home/moved-page.md`).
- [ ] H-1 guard: a fix whose emitted target STILL does not resolve must
      not be written — demote it to `unfixable` instead. This converts any
      future writer/resolver asymmetry from corruption into a visible
      count.
- [ ] H-2: exclude three contexts from auto-link candidate matching:
      inside a markdown link destination, inside a bare URL (reuse the
      MD034-style URL boundary scan), and inside an existing link's label
      text. Keep the current inline-code and whole-label exclusions. Unit
      tests per context + the `net` repro as e2e.
- [ ] M-1: the cross-vault basename fallback must not be labeled
      `LinkCaseMismatch`, must not claim confidence 1.0, and must not land
      in the default apply bucket. Either retire it or move it behind the
      existing fuzzy gate (`--apply-fuzzy` + `--min-confidence`) with an
      honest strategy name and scaled confidence.
- [ ] Conformance fixture (regression gate for the whole class): a scratch
      corpus with site-absolute, relative, `../`, and URL-adjacent links;
      e2e applies every proposed fix and asserts the broken count
      MONOTONICALLY DECREASES and no non-broken link changed. Wire into
      the normal test suite, not a separate gate.
- [ ] Re-run the report's GitHub Docs scenario on a scratch copy (copy
      3-4 top-level dirs): `links fix --apply` must strictly reduce the
      broken count and touch only files containing fixable links. Record
      before/after counts in this file.
- [ ] CHANGELOG entries under Fixed; update links docs if behavior wording
      changes.

## Acceptance criteria

- [ ] Minimal H-1 repro: after `links fix --apply`, the rewritten link
      resolves and a re-run reports 0 fixable
- [ ] H-2 repro: `links auto --apply` on the `net` vault rewrites ONLY the
      bare-word mention; both URLs and the existing link label are
      untouched
- [ ] No fix strategy outside the fuzzy gate can rewrite a link to a
      target that does not resolve
- [ ] GitHub Docs scratch copy: broken count strictly decreases under
      `links fix --apply`
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- `<dir>/index.md` resolution (F-1) — that is
  [[iteration-203-index-md-resolution]]; here only ensure fixes never make
  things worse in its absence.
- The `links` 12.7s perf issue on GitHub Docs — profile separately.
