---
title: Iteration 203 — resolve directory link targets to <target>/index.md
type: iteration
date: 2026-08-18
status: completed
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

## Tasks [8/8]

- [x] Core rule in the shared resolver (single entry point — do NOT fork
      per-caller logic; iter-189 collapsed the resolvers, keep it that
      way). Unit tests: `/foo`, `foo`, `/foo/`, `foo/` with and without
      `foo/index.md`; `foo` when BOTH `foo.md` and `foo/index.md` exist
      (file wins — document the precedence); case variants.
- [x] Thread through the index path; bump index data only if required
      (prefer derivable-at-query like iter-190's fail-safe).
- [x] `backlinks` counts directory-target links for the index file.
- [x] HYALO006 and `find --broken-links` stop flagging resolvable
      directory targets; `broken_anchor` works against the index file's
      headings.
- [x] `mv` rewrites `/foo`-style inbound links when `foo/index.md` moves
      — WITHOUT reintroducing L-11 (do not inject the site prefix or
      append `.md` to spellings that lacked them; preserve the original
      form).
- [x] UX-4: `hyalo config` shows the EFFECTIVE site_prefix (auto-derived
      value, marked as derived).
- [x] Measure on MDN (read-only, snapshot index in scratch): record
      before/after broken-link counts and `backlinks` results for
      `web/api/document/index.md` in this file. Expect broken to drop
      from ~49.7k to the low thousands.
- [x] Docs: links documentation resolution-order section, CHANGELOG
      (Added), README only if its prose becomes wrong.

## Measurements (MDN, read-only)

Corpus: `~/devel/mdn/files/en-us` (14,375 `.md` files). Baseline binary
`hyalo 0.20.0` (homebrew), candidate `target/release/hyalo` at this branch.
MDN publishes `web/api/document/index.md` as the URL `/en-US/docs/Web/API/Document`,
so the runs pass `--site-prefix "en-US/docs"`.

| Measurement | before (0.20.0) | after (iter-203) |
| --- | --- | --- |
| `links` broken | 49,703 | **509** (−99.0%) |
| `links` unfixable | 49,703 | 509 |
| `links` case_mismatches | 0 | 49,194 |
| `links` fixable | 0 | 0 |
| `backlinks web/api/document/index.md` | 0 | **13** (9 files) |

The 13 backlinks match a `grep -rlE "\(/en-US/docs/Web/API/Document[)#]"`
cross-check (9 files). The 49,194 case mismatches are the directory targets
now *resolving*: MDN writes `Web/API/Document` while the files are lowercase
on disk, so the case index answers with the canonical path. `fixable` stays 0,
so `links fix --apply` writes nothing — no bulk-rewrite risk.

Two findings worth their own follow-ups, both out of scope here:

- **Site-prefix stripping is case-sensitive and single-guess.** With the
  auto-derived prefix (`en-us`, from the directory name) MDN still reports
  49,703 broken links, because its URL prefix is the two-segment, differently
  cased `en-US/docs`. Nothing resolves until the prefix is passed by hand.
- **`links` on MDN takes ~80 s** — the known perf item, unchanged here.

## Acceptance criteria [5/5]

- [x] The F-1 fixture (A-E link matrix from the report) resolves
      `/foo`, `foo`, `/foo/`; `/bar/page` and `/foo/index` keep working
- [x] MDN broken-link count drops by >90%; `backlinks` on
      `web/api/document/index.md` returns non-zero
- [x] `foo.md` beats `foo/index.md` when both exist, and the precedence
      is documented
- [x] No `mv` rewrite changes a link's spelling style (prefix/extension)
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Making site-absolute targets configurable beyond the existing
  site_prefix semantics.
- The `links` 12.7s perf issue — separate profiling task.
- Sequencing: run AFTER [[iteration-200-links-apply-integrity]] so the
  conformance fixture from 200 guards this change too.
