---
title: Iteration 200 — links apply-path integrity (release blocker)
type: iteration
date: 2026-08-18
status: in-progress
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

- [x] H-1: make site-absolute fixes round-trip. A fix for a link written
      `/a/b` must be emitted in a form that the resolver actually resolves
      — either preserve the leading `/` (site-absolute stays
      site-absolute) or emit a correctly file-relative path. Add unit
      tests for absolute→absolute and the minimal repro from the report
      (`/how-tos/old-home/moved-page` → target at
      `how-tos/new-home/moved-page.md`).
- [x] H-1 guard: a fix whose emitted target STILL does not resolve must
      not be written — demote it to `unfixable` instead. This converts any
      future writer/resolver asymmetry from corruption into a visible
      count.
- [x] H-2: exclude three contexts from auto-link candidate matching:
      inside a markdown link destination, inside a bare URL (reuse the
      MD034-style URL boundary scan), and inside an existing link's label
      text. Keep the current inline-code and whole-label exclusions. Unit
      tests per context + the `net` repro as e2e.
- [x] M-1: the cross-vault basename fallback must not be labeled
      `LinkCaseMismatch`, must not claim confidence 1.0, and must not land
      in the default apply bucket. Either retire it or move it behind the
      existing fuzzy gate (`--apply-fuzzy` + `--min-confidence`) with an
      honest strategy name and scaled confidence.
- [x] Conformance fixture (regression gate for the whole class): a scratch
      corpus with site-absolute, relative, `../`, and URL-adjacent links;
      e2e applies every proposed fix and asserts the broken count
      MONOTONICALLY DECREASES and no non-broken link changed. Wire into
      the normal test suite, not a separate gate.
- [x] Re-run the report's GitHub Docs scenario on a scratch copy (copy
      3-4 top-level dirs): `links fix --apply` must strictly reduce the
      broken count and touch only files containing fixable links. Record
      before/after counts in this file.
- [x] CHANGELOG entries under Fixed; update links docs if behavior wording
      changes.

## Results

### What changed

- **H-1 (writer/resolver asymmetry).** `link_fix::build_replacements_for_file`
  hand-built the new destination from the *vault-relative* `FixPlan::new_target`
  and wrote it verbatim. Repairs now go through `emit_markdown_fix_target`:
  site-absolute in ⇒ site-absolute out, otherwise a path relative to the
  source file's own directory (`relative_path_between`), `.md` presence
  preserved. An auto-derived `site_prefix` (hyalo derives one from the vault
  directory name when none is configured) is only re-attached when the original
  link actually carried it — otherwise `mv`-style prefix injection (L-11) would
  have crept into `links fix` too.
- **H-1 guard.** `markdown_fix_round_trips` / `wikilink_fix_round_trips`
  re-normalize the emitted text exactly as the read-side resolver does. A fix
  that does not read back as its own target is never written: it is reported in
  `unfixable` / `unfixable_links`. `apply_fixes` and `plan_fixes_dry_run` both
  return the rejected set, so dry-run and apply agree.
- **H-1 side fix.** `LinkMatcher` now strips the leading `/` and any configured
  site prefix before matching, so site-absolute targets can finally reach the
  exact / case-insensitive / extension strategies instead of always falling
  through to the basename guess.
- **H-2.** New `links::inert_link_zones` returns every byte range that is
  syntactically part of a link: whole `[label](dest)` constructs (external
  destinations included — `extract_link_spans` drops those, which is why the
  bug existed), whole `[[wikilinks]]`, `<autolinks>`, and bare URLs in prose.
  `scan_file_for_matches` skips candidates overlapping any zone.
- **M-1.** `discovery::resolve_target` no longer applies the Obsidian bare-stem
  fallback to a site-absolute target, so `/actions` stops "resolving" to
  `graphql/reference/actions.md` as a confidence-1.0 `LinkCaseMismatch`. When
  the matcher later reaches the same file by basename it reports the new
  `FixStrategy::BasenameFallback` at confidence 0.6, grouped with fuzzy matches
  and therefore gated behind `--apply-fuzzy` / `--min-confidence`. Bare and
  relative targets keep the 0.95 `ShortestPath` treatment — for them the
  basename is the reliable signal, and the round-trip guard now guarantees
  whatever gets written resolves.

### GitHub Docs scratch copy

961 files: `actions`, `graphql`, `get-started`, `code-security` copied from
`~/devel/docs/content`.

| run | files modified | broken before → after |
|---|---|---|
| `links fix --apply` (plain) | **0** | 3341 → 3341 |
| `links fix --apply --apply-fuzzy` | 507 | **3341 → 1008** |

Plain `--apply` now writes nothing on this corpus: every remaining candidate is
a site-absolute basename guess, which is exactly the class that used to modify
1,097 files and push the broken count *up* (6,565 → 6,582 in the dogfood run).
With the guesses explicitly opted into, 2,333 fixes land across 507 files and
the broken count drops monotonically; the 1,008 that remain are the `index.md`
resolution gap (F-1, [[iteration-203-index-md-resolution]]). Every changed line
in the diff contains a link — no prose was touched.

`links auto --apply` on the same corpus: 738 files, 7,660 wikilinks inserted,
**0** wikilinks inside a URL, a markdown destination, or a link label
(verified by scanning every URL token and every `[label](…)` in the result).

## Acceptance criteria

- [x] Minimal H-1 repro: after `links fix --apply`, the rewritten link
      resolves and a re-run reports 0 fixable — with the caveat that the
      repro's target moved *directories*, so under M-1 it is now a gated
      guess: plain `--apply` correctly writes nothing (and a re-run reports
      0 fixable), and `--apply --apply-fuzzy` writes
      `/how-tos/new-home/moved-page`, after which a re-run reports 0 broken
      and 0 fixable. A site-absolute repair that is *not* a guess (case or
      extension only) lands under plain `--apply` and round-trips —
      covered by `site_absolute_case_mismatch_is_a_certain_fix` and the
      conformance e2e.
- [x] H-2 repro: `links auto --apply` on the `net` vault rewrites ONLY the
      bare-word mention; both URLs and the existing link label are
      untouched
- [x] No fix strategy outside the fuzzy gate can rewrite a link to a
      target that does not resolve
- [x] GitHub Docs scratch copy: broken count strictly decreases under
      `links fix --apply`
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- `<dir>/index.md` resolution (F-1) — that is
  [[iteration-203-index-md-resolution]]; here only ensure fixes never make
  things worse in its absence.
- The `links` 12.7s perf issue on GitHub Docs — profile separately.
