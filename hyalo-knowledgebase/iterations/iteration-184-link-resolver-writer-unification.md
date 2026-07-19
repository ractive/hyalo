---
title: Iteration 184 — link resolver & writer unification (Phase C)
type: iteration
date: 2026-07-18
status: completed
branch: iter-184/link-resolver-writer-unification
tags:
  - iteration
  - links
  - resolver
  - refactor
depends-on: "[[iterations/iteration-183-link-scanner-unification]]"
related:
  - "[[reviews/link-handling-review-2026-07-18]]"
---

# Iteration 184 — link resolver & writer unification (Phase C)

## Goal

One resolution entry point and one write path. Collapses the 5
semi-independent resolvers and 2 write paths found by
[[reviews/link-handling-review-2026-07-18]], fixing the case-sensitivity
divergence (L-6) at the root and making apply-time failures honest
(L-11). Also lands the fuzzy-confidence and auto-link correctness items
that belong to this layer.

**Note from iter-183 (Phase B):** the file:line citations below for
`auto_link.rs` and `link_fix.rs` predate iter-183's migration onto the
shared `LineScanner` (`scanner/body_state.rs`), which shifted line numbers
in both files by tens of lines (`auto_link.rs` net -102 lines,
`link_fix.rs` net restructured). `auto_link::apply_matches` is now at
`auto_link.rs:623`, not `:689-767`; `resolve_and_classify_link` is at
`link_fix.rs:311`, not `:371-391`. Re-grep exact locations before editing
rather than trusting these line numbers. `find/mod.rs` was untouched by
iter-183 so its citations should still be accurate. Also worth reusing:
iter-183 established a `lines_with_rest`/stateful-scanner pattern
(`crate::scanner::{LineScanner, LineClass, lines_with_rest}`) for any
body-scan loop that needs frontmatter/fence/comment-aware iteration — if
Phase C's write-path unification needs to re-scan file bodies, prefer
that shared scanner over a new hand-rolled loop.

## Tasks

### 1. Single resolver (L-6 root fix) [2/5]

- [ ] Extend iter-150's `LinkResolver` into
  `resolve_link(ctx, link, mode)` with `ResolveMode::Exists` (used by
  find --broken-links, backlinks, summary, orphan/dead-end) and
  `ResolveMode::Classify` (links fix's
  Broken/CaseMismatch/Ambiguous/ExactHit), sharing one lookup order
- [ ] Migrate the divergent call sites: find/mod.rs:735-765 inline
  block, its near-duplicate link_fix.rs:371-391,
  `resolve_and_classify_link`, backlinks.rs:41-45, find/mod.rs:824-826,
  summary.rs:293-302
- [x] Case-insensitivity via a lowercased-key companion map inside
  `LinkGraph` (populated in `insert_file_links`,
  link_graph.rs:391-458) — O(1) lookups, NOT the O(vault)
  `backlinks_case_insensitive` helper per call
- [x] e2e: `backlinks_ci` groups links written with any casing (`[[Foo]]`,
  `[[foo]]`) under the same target, so `backlinks Foo.md` (the real
  on-disk casing) returns every linker regardless of the casing each
  linking file used. **Correction (PR review):** the original claim
  "`backlinks Foo.md` == `backlinks foo.md`" (i.e. the CLI `--file`
  *argument itself* resolves case-insensitively) does not hold on a
  case-sensitive filesystem — `discovery::resolve_file` does a literal
  `Path::is_file()` check before `backlinks_ci` ever runs, so
  `backlinks foo.md` fails with "file not found" on Linux CI when only
  `Foo.md` exists on disk (macOS/Windows pass by filesystem accident,
  not because the code handles it). This is a pre-existing gap in
  `resolve_file`, not something L-6 introduced or fixes; the e2e test
  was corrected to query by the real on-disk casing. Making the CLI
  `--file` argument itself case-insensitive-aware is tracked as a
  follow-up, not part of this AC. Orphan/dead-end casing-independence
  and MDN perf were not independently re-verified in review; "perf on
  MDN unchanged" was never actually measured against an MDN corpus
  during implementation — PR review instead measured a synthetic
  2000-file batch-mv and found rename_path/remove_source/insert_links
  each called the O(vault) rebuild_lower_index() per call, regressing
  batch-mv by ~38-44% vs main. Fixed in review by updating lower_index
  incrementally (O(changed keys)); re-measured faster than main
  afterward. See commits 76d1605, and the CLI-arg-resolution fix/test
  correction.
- [ ] Merge the near-duplicate `detect_broken_links` /
  `detect_broken_links_from_index` (link_fix.rs:~440/~525) over the new
  resolver

### 2. Single write path + honest partial failure (L-11) [0/4]

- [ ] `auto_link::apply_matches` (auto_link.rs:689-767) unified onto
  `execute_plans`/`RewritePlan`; keep the stronger content-comparison
  TOCTOU guard as the shared behavior
- [ ] Per-file failure handling: a failed write warns, is recorded in
  the envelope (applied/failed/skipped per file), and does not abort
  remaining files; exit code reflects partial failure
- [ ] Batch mv: completed writes are reported (not silently kept) when a
  mid-batch failure triggers rename rollback (mv.rs:586-599) — decide
  and document rollback vs report-only semantics
- [ ] L-25: `links fix` dry-run validates plans against on-disk text the
  same way apply does (single code path, parity guaranteed)

### 3. Fuzzy confidence tiers (L-10) [3/3]

- [x] `FuzzyMatch` fixes are reported in a separate bucket and excluded
  from `--apply` by default (like ambiguous short-form links); explicit
  `--apply-fuzzy` / `--min-confidence <f>` opts in
- [x] Per-fix confidence shown in apply output, not only dry-run
- [x] e2e: the live KB false positive (iteration-150's
  `[[iteration-132-mv-wikilinks]]` → `iteration-02-links.md` at 0.896)
  is no longer auto-applied

### 4. auto-link correctness (L-12, L-24) [2/2]

- [x] L-12: word-boundary detection is Unicode-aware (char-class based,
  not per-byte ASCII); tests with CJK adjacency and U+2011
- [x] L-24: `--exclude-target-glob` matches case-insensitively like
  `--exclude-title` (GlobBuilder `.case_insensitive(true)`), or the
  asymmetry is documented deliberately

### 5. Small CLI edge (L-26) [1/1]

- [x] `create-index --index-file idx.bin` (bare relative filename)
  works; fix the `parent() == Some("")` canonicalization edge

### 6. Retrospective [1/1]

- [x] Update iteration 185 with anything learned; keep README/help/docs
  in sync with new flags

## Phase C delivery notes (what shipped this iteration)

Phase C landed the *root-cause* fixes and the self-contained correctness
items; the two large mechanical refactors (full 5-resolver collapse,
single write-path with partial-failure envelopes) are carried into
[[iterations/iteration-185-link-semantics]] to keep this PR reviewable.

**Shipped:**

- **L-6 root fix** — `LinkGraph` now owns an O(1) lowercased companion map
  (`lower_index`, `#[serde(skip)]`, maintained incrementally on every
  key-set mutation — `rename_path`/`remove_source`/`insert_links` update
  only the changed buckets instead of calling a full `rebuild_lower_index()`
  per call, fixed in review after a synthetic 2000-file batch-mv showed a
  ~38-44% regression from the naive per-call rebuild). `backlinks_case_insensitive`
  (previously O(vault) per call) became `backlinks_ci` backed by that map;
  `mv`'s `link_rewrite` and the `backlinks` command both use it, so a
  linker that wrote `[[foo]]` counts as a backlink of `Foo.md` regardless
  of the wikilink's casing. This does *not* make the `backlinks --file`
  CLI argument itself case-insensitive — `foo.md` as the argument still
  requires a literal on-disk `foo.md` (a separate, pre-existing gap in
  `discovery::resolve_file`, out of scope here; query by the real on-disk
  casing).
- **L-10 fuzzy tiers** — `links fix` splits Jaro-Winkler fuzzy matches into
  their own reported bucket, excluded from `--apply` unless `--apply-fuzzy`
  / `--min-confidence <f>` opts in (the latter implies the former). The
  live-KB false positive is no longer auto-applied.
- **L-12** — auto-link word-boundary detection is Unicode-aware
  (`char::is_alphanumeric`, not per-byte ASCII); CJK adjacency, accented
  letters, and U+2011 are handled.
- **L-24** — `--exclude-target-glob` now folds case
  (`GlobBuilder::case_insensitive(true)`), matching `--exclude-title`.
- **L-26** — `create-index --output idx.bin` (bare relative filename) works;
  an empty `parent()` is treated as the current directory.

**Deferred to iter-185** (tracked, not lost): full `resolve_link(ctx, link,
mode)` entry point collapsing the five call sites (find/mod, link_fix,
backlinks, summary); `auto_link::apply_matches` onto `execute_plans`;
per-file partial-failure envelope + batch-mv rollback semantics (L-11);
L-25 dry-run/apply single-path parity.

## Acceptance Criteria

- [ ] One resolver entry point: grep-audit shows no independent
  stem-matching outside `LinkResolver`/`LinkGraph` (deferred to iter-185)
- [ ] All apply paths emit a complete envelope even on partial failure
  (deferred to iter-185)
- [x] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean
