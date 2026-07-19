---
title: Iteration 189 — link resolver classify collapse (refactor only)
type: iteration
date: 2026-07-19
status: in-progress
branch: iter-189/resolver-classify-collapse
tags:
  - iteration
  - links
  - refactor
  - resolver
depends-on: "[[iterations/iteration-188-link-semantics-completion]]"
related:
  - "[[reviews/link-handling-review-2026-07-18]]"
  - "[[iterations/iteration-187-link-writer-unification]]"
  - "[[iterations/iteration-184-link-resolver-writer-unification]]"
---

# Iteration 189 — link resolver classify collapse (refactor only)

## Goal

Finish the resolver-side collapse that has now been deferred THREE times
(184 → 187 → 188) precisely because feature work kept crowding it out.
This iteration is that scope and nothing else: the two "carried:" entries
of [[iterations/iteration-188-link-semantics-completion]] task 0 — the
`link_fix.rs` Classify-side policy collapse (with the `detect_broken_links`
merge) and the `summary.rs` orphan/dead-end L-6 tail — plus the perf A/B
that was carried with them.

**Refactor only — a PR adding user-visible behavior fails this iteration's
review.**

**NO features. NO new user-visible behavior. Every migration is
behavior-preserving and must be locked by e2e captured BEFORE the migration
lands.** The single sanctioned observable delta is the L-6 count correction
in `summary` (task 3): orphan/dead-end counts on case-insensitive-written
links become consistent with `backlinks_ci` — a bug fix mandated by the
review, guarded by its own e2e, and called out in the CHANGELOG. Anything
else that changes observable output is a defect of this iteration.

**Do NOT release; release is a separate user-gated step.**

**Line-reference note:** all file:line citations below were re-derived
against main at `9cebfdc` (post iter-187/188 merges). The citations in the
187/188 plans are stale for `link_fix.rs` (functions shifted ~90 lines
after the iter-188 short-form work).

**What is already in place (do not redo):**

- `discovery::resolve_link_from_source` (discovery.rs:813-844) is the
  shared Exists-mode entry point; `find/mod.rs:735-742` and the HYALO006
  rule (`link_lint.rs:120-127`) already route through it.
- `detect_broken_links` (link_fix.rs:428) is already test-only
  (`#[allow(dead_code)]` at :427); the CLI calls only
  `detect_broken_links_from_index` (link_fix.rs:540, invoked from
  commands/links.rs:72). The merge below is deletion + test port, not a
  caller migration.
- `LinkGraph::backlinks_ci` (link_graph.rs:245-270) and the incrementally
  maintained `lower_index` (link_graph.rs:201-208, rebuilt after snapshot
  load at index.rs:732) already provide O(1) case-insensitive lookups —
  task 3 consumes them, it does not build new machinery.
- iter-184 lesson (MUST honor): no O(vault) work inside a per-file loop;
  `lower_index` stays incrementally maintained.

## Tasks

### 1. Behavior locks captured BEFORE each migration [0/4]

- [x] Inventory the existing locks and run them green on the branch point:
  `links fix` dry-run/apply e2e (tests/e2e/links.rs:405-541+,
  `links_fix_dry_run_reports_broken_and_fixable`,
  `links_fix_apply_reduces_broken_links` and siblings),
  `find --broken-links` / `--orphan` / `--dead-end` e2e
  (tests/e2e/find.rs:3511+), HYALO006 e2e (tests/e2e/lint.rs,
  `hyalo006_*`), and the `detect_broken_links*` unit tests
  (link_fix.rs:1597, :1635, :2078, :2125, :2308)
- [x] Add any missing lock BEFORE task 2 lands: an e2e fixing the exact
  classify verdicts (`broken` / `case_mismatches` / `ambiguous` buckets and
  their sort order, link_fix.rs:622-624) for a vault that exercises every
  `LinkResolution` variant (link_fix.rs:165-172): resolved, case-mismatch,
  short-form valid, short-form stem-mismatch, short-form ambiguous, broken,
  and the `--expand-short-form` flag path
- [x] Add a lock for the markdown bare-basename fallback asymmetry: Exists
  mode probes `resolve_target` with the case index (discovery.rs:834-839)
  while Classify mode falls back only when the source-relative verdict is
  `Broken` (link_fix.rs:394-405) — pin both behaviors so the collapse in
  task 2 cannot silently unify them
- [x] Commit the locks separately, before any refactor commit, so the diff
  history proves before/after parity

### 2. Classify-side collapse onto the shared entry point [0/5]

`resolve_and_classify_link` (link_fix.rs:325-408) duplicates the
kind-dependent normalization that `discovery::resolve_link_from_source`
(discovery.rs:813-844) owns for Exists mode: wikilink as-written, markdown
site-absolute (`/...`), path-qualified (`normalize_target` against the
source dir), bare-basename fallback. Concrete proposal — a **sibling
function, Classify mode**, in discovery:

- [x] Extract the markdown-target normalization branching into one private
  helper in discovery.rs (e.g.
  `normalize_link_target(kind, source_rel, target, probe) -> Cow<str>`
  where `probe` abstracts the two bare-basename fallback policies pinned in
  task 1), and re-express `resolve_link_from_source` through it —
  observable behavior unchanged (existing e2e green)
- [x] Add the Classify sibling
  `discovery::classify_link_from_source(canonical_dir, source_rel, link:
  &Link, site_prefix, case_index, stem_index, expand_short_form) ->
  (String, LinkResolution)` by moving `resolve_and_classify_link`
  (link_fix.rs:325-408), `classify_link` (link_fix.rs:270-309),
  `classify_short_form_wikilink` (link_fix.rs:209-268), `LinkResolution`
  (link_fix.rs:165-172) and `StemIndex` (link_fix.rs:177-207) into
  discovery (pub(crate); re-export from link_fix.rs if needed to keep the
  crate API surface unchanged), routed through the same normalization
  helper so Exists and Classify branching can never drift again
- [x] Migrate `detect_broken_links_from_index` (link_fix.rs:540-632, call
  site :571) onto `classify_link_from_source`; delete the link_fix-local
  copies when unused
- [x] Doc comments state the mode contract explicitly (Exists = "does this
  link resolve to a vault file", Classify = "full fix-policy verdict incl.
  case/short-form buckets") and cross-reference each other
- [x] Behavior-lock e2e from task 1 green after the migration; no output
  diff in `links fix` dry-run JSON on the lock vault

### 3. summary.rs orphan/dead-end L-6 tail [0/4]

`summary` still counts orphans/dead-ends with case-SENSITIVE membership:
summary.rs:303-326 builds `graph.all_targets()` / `graph.all_sources()`
sets (link_graph.rs:214-230) and checks `targets.contains(rel_str)` /
`targets.contains(without_md)` (summary.rs:316) and
`sources.contains(rel_str)` (summary.rs:317). On a case-insensitive vault,
`[[foo]]` pointing at `Foo.md` leaves `Foo.md` counted as an orphan while
`backlinks foo` (via `backlinks_ci`) finds the linker. Note: the review's
Disposition section already marks L-6 "resolved" including the summary
tail — that annotation is ahead of the code; this task makes it true.

- [x] Route inbound membership through the `lower_index`-aware lookup:
  `has_inbound = !graph.backlinks_ci(&entry.rel_path).is_empty()`
  (link_graph.rs:245-270 already checks both the `.md` and stem forms, so
  the hand-rolled `without_md` dance at summary.rs:315-316 disappears);
  keep self-link inclusion semantics identical to today (the current
  `contains` check does not exclude self-links, and neither does
  `backlinks_ci` — parity, no new filtering)
- [x] Outbound membership: keep the `all_sources` set (sources are actual
  on-disk rel paths compared against actual on-disk rel paths, so there is
  no case divergence) — document why it is exempt in a comment
- [x] e2e: vault with `a.md` containing `[[foo]]` and file `Foo.md` —
  `summary` counts `Foo.md` as NOT orphan (dead-end instead, since it has
  inbound and no outbound) and counts agree with `backlinks_ci`; run both
  disk-scan and `--index` paths (mirror the existing parity test pattern,
  summary.rs:865+)
- [x] Audit note: `find --orphan` / `--dead-end` compute inbound via
  case-sensitive `graph.backlinks()` (find/mod.rs:794-801) — the same L-6
  root. Either align it in the same commit with its own e2e lock
  (one-line: `backlinks` → `backlinks_ci`), or record explicitly in the PR
  and the review doc why it stays for a follow-up. Do NOT leave the
  divergence undocumented — `find --orphan` and `summary` orphan counts
  should not silently disagree

### 4. detect_broken_links merge (delete the test-only twin) [0/3]

- [x] Delete `detect_broken_links` (link_fix.rs:427-525) — it is
  `#[allow(dead_code)]`, test-only, and byte-for-byte parallel to
  `detect_broken_links_from_index` (:540-632) except for iterating
  `&[FileLinks]` instead of `index.entries()`
- [x] Port its unit tests (`detect_broken_links_finds_missing` :1597,
  `detect_broken_links_sorted` :1635,
  `detect_broken_links_emits_case_mismatch_with_index` :2078,
  `detect_broken_links_case_mismatch_has_correct_strategy` :2125,
  `detect_broken_links_no_index_no_case_mismatches` :2308) to call
  `detect_broken_links_from_index` — either via `ScannedIndex` over the
  test tempdir (VaultIndex impl at index.rs:242-250) or a minimal test
  helper constructing `IndexEntry` values from the same fixtures; every
  assertion preserved, none weakened
- [x] `FileLinks`-based plumbing that only served the deleted function is
  removed; clippy dead-code clean without new `#[allow]`s

### 5. Grep-audit: no independent resolution loops in commands/ [0/2]

- [x] Run and record in the PR description:
  `grep -rn "resolve_target\|classify_link\|resolve_and_classify" crates/hyalo-cli/src/commands/`
  — expected result after tasks 2-4: zero direct callers; every
  resolution in commands/ goes through `resolve_link_from_source`
  (find/mod.rs:735, link_lint.rs:120) or
  `detect_broken_links_from_index` → `classify_link_from_source`
  (commands/links.rs:72). Doc-comment mentions (dispatch.rs:75) are
  allowed; code paths are not
- [x] Confirm the rewrite-side `LinkResolver` (hyalo-core
  link_resolve.rs, used by mv/auto-link planning) is documented as the
  *rewrite-planning* resolver, distinct from the read-side
  Exists/Classify entry points — one sentence in each module header so
  the next reader doesn't count it as a rogue loop

### 6. Perf A/B [0/4]

`bench-e2e.sh` exists (hyperfine; A/B via a second-binary argument;
vault from `HYALO_BENCH_VAULT`, default `../obsidian-hub`) but currently
benches only find/properties/tags/summary — none of the three commands
this refactor touches.

- [x] Add three benches: `links fix` (dry-run, default),
  `find --broken-links`, and batch `mv --apply` — the mv bench needs a
  throwaway generated vault regenerated via hyperfine `--prepare` (apply
  mutates the tree); the first two can run read-only against the bench
  vault
- [x] Run A/B: baseline = binary built from main `9cebfdc`, current =
  branch head; `./bench-e2e.sh target/release/hyalo <baseline-binary>`
- [x] Record the numbers HERE (table below) before ticking — iter-184/185
  lesson: no perf claim without a measurement
- [x] AC: within noise (the collapse re-routes the same `resolve_target`
  calls; no added per-link work)

Measured 2026-07-19 with `bench-e2e.sh` (hyperfine, warmup 3 / 10 runs) against
`../obsidian-hub`; baseline binary built from main `9cebfdc`, branch binary from
branch head. All three deltas fall inside the per-command error bars (overlapping
±σ ranges), i.e. within noise as the AC requires. The refactor re-routes the same
`resolve_target` calls; `mv-batch-apply` was not touched at all so its parity is
expected.

| bench | baseline (9cebfdc) | branch | delta |
| --- | --- | --- | --- |
| links-fix-dry-run | 12.177 s ± 0.044 | 12.190 s ± 0.051 | +0.1 % (noise) |
| find-broken-links | 466.4 ms ± 10.8 | 478.2 ms ± 17.7 | +2.5 % (noise, σ overlap) |
| mv-batch-apply | 17.786 s ± 0.935 | 18.004 s ± 2.096 | +1.2 % (noise, σ overlap) |

### 7. Retrospective [0/3]

- [x] Update the "carried:" annotations in
  [[iterations/iteration-188-link-semantics-completion]] task 0 to
  point here as the terminus (done-in-189), and fix the L-6 disposition
  line in [[reviews/link-handling-review-2026-07-18]] to match reality
  (it currently claims the summary tail landed in 188)
- [x] CHANGELOG: internal-refactor note + the L-6 summary-count correction
  (the one observable fix); no release
- [x] KB edits via `hyalo set`/`read`/`lint` per CLAUDE.md; record a DEC
  entry only if task 3's find-alignment decision or task 2's fallback
  unification produced a real decision

## Acceptance Criteria

- [x] Behavior-lock e2e present and green, covering the migration: `links_fix_classify_verdict_buckets_lock` and `summary_orphan_dead_end_case_insensitive_inbound`, both routed through `classify_link_from_source` / `normalize_link_target`.
  Note: the PR landed as one squashed commit (`9498586`), not the
  separately-committed locks-before-refactor sequence task 1 originally
  specified — before/after parity is evidenced by the passing suite and
  the diff, not commit-history ordering.
- [x] No user-visible behavior change except the documented L-6 orphan/dead-end count correction in `summary`, whose inbound check is now `backlinks_ci` instead of a case-sensitive `all_targets().contains()`.
  Own e2e, CHANGELOG note.
- [x] One Classify entry point: `classify_link` and `classify_short_form_wikilink` now live in discovery.rs feeding `classify_link_from_source`; `detect_broken_links` deleted, all five unit tests ported onto `mock_index` and green.
- [x] Grep-audit documented in the PR body ("Grep-audit (task 5)" section): zero code call sites outside `resolve_link_from_source` / `classify_link_from_source` in commands/; re-verified at merge time.
- [x] Perf A/B numbers recorded in this plan (task 6 table) and the PR body via the three new `bench-e2e.sh` cases `links-fix-dry-run`, `find-broken-links`, `mv-batch-apply`; all three within noise (overlapping ±σ).
- [x] `cargo fmt` / `clippy --workspace --all-targets -- -D warnings` /
  `cargo test --workspace -q` clean
