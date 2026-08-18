---
title: Iteration 203 — resolve directory link targets to <target>/index.md
type: iteration
date: 2026-08-18
status: planned
branch: iter-203/index-md-resolution
tags:
  - iteration
  - links
  - feature
related:
  - "[[dogfood-results/dogfood-v0210-pre-release-iters-191-198]]"
---

# Iteration 203 — resolve directory link targets to <target>/index.md

## Goal

Teach link resolution that a target denoting a directory resolves to that
directory's `index.md` (dogfood F-1). Today `/foo`, `foo`, and even `/foo/`
are "broken" when `foo/index.md` exists — which makes MDN read as 49,703 of
49,705 links broken (99.996%) and `backlinks` return 0 for its most-linked
pages, and it is the trigger M-1 exploited. Highest-leverage single fix for
directory-index corpora (MDN, GitHub Docs, most static-site docs).

**Do NOT release; release is a separate user-gated step.**

## Context

Repros in [[dogfood-results/dogfood-v0210-pre-release-iters-191-198]] (F-1,
UX-4). Anchors at `931a226`:

- Resolution lives in `hyalo-core/src/discovery.rs` (`resolve_target`,
  ~line 1350ff: existing `.md`-append fallback is the model — the new rule
  is a sibling fallback) and `link_resolve.rs`; the anchor-aware and index
  (`--index`) paths must both learn the rule, like iter-190 did for
  fragments.
- Resolution order to implement, after the existing exact and `.md`-append
  attempts fail: `<target>/index.md` (case-handled like everything else).
  A trailing-slash target (`/foo/`) skips the `.md`-append attempt
  entirely — it is unambiguously a directory reference.
- Site-absolute interaction: `/foo` under `--site-prefix`/auto-derived
  prefix resolves relative to the vault/prefix root, then the same
  directory fallback applies. `hyalo config` currently prints
  `site_prefix: (none)` instead of the auto-derived effective value
  (UX-4) — surface it, since it decides what `/foo` means.
- Downstream surfaces that must agree: `find --broken-links`, `links`
  (broken/fixable buckets), `backlinks` (a link to `/foo` is a backlink
  of `foo/index.md`), HYALO006, `mv` (renaming `foo/index.md` or the
  `foo/` dir rewrites `/foo`-style inbound links), anchors
  (`/foo#section` checks headings of `foo/index.md`).

## Tasks

- [ ] Core rule in the shared resolver (single entry point — do NOT fork
      per-caller logic; iter-189 collapsed the resolvers, keep it that
      way). Unit tests: `/foo`, `foo`, `/foo/`, `foo/` with and without
      `foo/index.md`; `foo` when BOTH `foo.md` and `foo/index.md` exist
      (file wins — document the precedence); case variants.
- [ ] Thread through the index path; bump index data only if required
      (prefer derivable-at-query like iter-190's fail-safe).
- [ ] `backlinks` counts directory-target links for the index file.
- [ ] HYALO006 and `find --broken-links` stop flagging resolvable
      directory targets; `broken_anchor` works against the index file's
      headings.
- [ ] `mv` rewrites `/foo`-style inbound links when `foo/index.md` moves
      — WITHOUT reintroducing L-11 (do not inject the site prefix or
      append `.md` to spellings that lacked them; preserve the original
      form).
- [ ] UX-4: `hyalo config` shows the EFFECTIVE site_prefix (auto-derived
      value, marked as derived).
- [ ] Measure on MDN (read-only, snapshot index in scratch): record
      before/after broken-link counts and `backlinks` results for
      `web/api/document/index.md` in this file. Expect broken to drop
      from ~49.7k to the low thousands.
- [ ] Docs: links documentation resolution-order section, CHANGELOG
      (Added), README only if its prose becomes wrong.

## Acceptance criteria

- [ ] The F-1 fixture (A-E link matrix from the report) resolves
      `/foo`, `foo`, `/foo/`; `/bar/page` and `/foo/index` keep working
- [ ] MDN broken-link count drops by >90%; `backlinks` on
      `web/api/document/index.md` returns non-zero
- [ ] `foo.md` beats `foo/index.md` when both exist, and the precedence
      is documented
- [ ] No `mv` rewrite changes a link's spelling style (prefix/extension)
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Making site-absolute targets configurable beyond the existing
  site_prefix semantics.
- The `links` 12.7s perf issue — separate profiling task.
- Sequencing: run AFTER [[iteration-200-links-apply-integrity]] so the
  conformance fixture from 200 guards this change too.
