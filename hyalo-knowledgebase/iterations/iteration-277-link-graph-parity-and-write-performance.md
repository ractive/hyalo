---
title: "Iteration 277 — Link-graph parity, links fix reporting, hint threading and write performance"
type: iteration
date: 2026-09-05
tags: [iteration, links, performance, index, hints, dogfooding]
status: completed
branch: iter-277/link-graph-parity-and-write-performance
priority: 3
related:
  - "[[dogfood-results/dogfood-v0220-post-batch-271-274]]"
  - "[[iterations/iteration-271-write-and-rewrite-safety]]"
  - "[[iterations/iteration-272-resolution-completeness]]"
  - "[[iterations/iteration-274-hints-help-and-contract-polish]]"
  - "[[decision-log]]"
---

# Iteration 277 — Link-graph parity, links fix reporting, hint threading and write performance

## Goal

Close the cost and consistency findings from
[[dogfood-results/dogfood-v0220-post-batch-271-274]] (BUG-13, 14, 15, 16, 17, 18, 24, 45, 46,
47; UX-6, 8, 9, 10, 11, 12, 13; G1, G2, G3, G6). Two of them are real performance regressions
introduced by the batch: every write now pays a per-file fsync (49 s for `lint --fix` on the
Hub, 25 s for one `mv`), and `site_prefix` resolution leaves the snapshot to stat the
filesystem once per link (indexed `summary` on MDN 0.55 s → 4.65 s). The rest are two commands
disagreeing about the same graph, `links fix` buckets that break their own contract, and hints
that drop the one flag that changes the answer.

Rules: **no new CLI flags** (the four gaps close as DEC won't-do, a `--jq` recipe, or a config
key, never a flag); measure before and after on the Hub and MDN and record the numbers in the
Outcome; WIP commit after each part; leftovers to `backlog/`.

## Part A — Write performance (BUG-14) [3/3]

- [x] PERF-1: profile one `lint --fix` on a Hub copy and one 2190-backlink `mv`; confirm the
      per-file temp+fsync+rename path from iteration 271 is the cost (report: 8–11 ms per
      file, 49 s / 25 s).
- [x] PERF-2: keep the atomicity guarantee per file but stop paying it serially: parallelise
      the rewrite phase of `lint --fix`, `mv`, `links fix --apply`, `set/append/remove --glob`
      and `properties rename`/`tags rename` with the same rayon pool the scan uses, and fsync
      once per directory at the end where the platform allows (document the Windows
      behaviour). Target: Hub `lint --fix` ≤ 10 s, the 2190-link `mv` ≤ 5 s, MDN bulk `set`
      ≥ 500 files/s.
- [x] PERF-3: a progress line on stderr (`-q`-silenced) once a write phase passes a few
      hundred files, so 49 s of silence never reads as a hang.

## Part B — `site_prefix` resolution from memory (BUG-13, 24) [2/2]

- [x] PREFIX-1: `classify_link`/site-absolute resolution consults the in-memory file set (the
      scan's or the snapshot's) instead of `Path::is_file()`; on the snapshot path no
      filesystem access at all. Measure MDN with `--site-prefix en-US/docs`: indexed `summary`
      4.65 s → target ≤ 0.8 s, `find --broken-links --count` 2.37 s → ≤ 0.6 s, `create-index`
      6.44 s → ≤ 4 s. Re-run the parity check (byte-identical JSON) afterwards.
- [x] PREFIX-2 (BUG-24): the snapshot header records `skipped` next to `excluded` so
      `summary --index` matches the disk scan (Hub 1, kepano 28) and keeps the skipped
      directory row.

## Part C — One graph, one answer (BUG-16, 45, 46; G3) [3/3]

- [x] GRAPH-1 (BUG-16): `summary`, `find --orphan` and `find --dead-end` share one edge
      definition. Decide in a DEC whether attachment links are edges (the report's 25 MDN
      files whose only outbound links are images); `find --help`'s "not graph edges" is the
      current promise, so the default choice is to make `summary` agree with it. Record the
      MDN numbers before and after.
- [x] GRAPH-2 (BUG-45): `links fix` JSON `broken_anchors` reports the count `find` computes
      (MDN 10929) and is documented in `links fix --help`, or the key is removed; never a
      constant 0.
- [x] GRAPH-3 (BUG-46): the "stripped 0 of N" and "skipped fuzzy scoring for N" warnings
      count the same set (49767 vs 49776 on MDN).
- **Deferred to backlog:** GRAPH-4 (G3): `--fields links` records carry `ambiguous: true`
      with `candidates` when a
      stem or alias collides, so ambiguous and missing can be told apart without `links fix`;
      `mv`, `backlinks` and HYALO006 read the same field.
      Remains open in [[backlog/link-ambiguity-as-a-first-class-link-field]].

## Part D — `links fix` reporting (BUG-17, 18; G2) [4/4]

- [x] FIX-1 (BUG-17): every `fuzzy_fixes[]` entry carries `emitted_target`, computed by the
      same function as the other buckets (`links fix --help` already promises it).
- [x] FIX-2 (BUG-18): fuzzy confidence drops below the floor when the runner-up scores within
      a margin (report: `Cat → CatMuse 0.87` with five `cat*` notes, `jamesb → jamesgreenblue
      0.885` with eight `james*`, `…/tabindex → global_attributes/index.md 0.9125` on a
      directory-index corpus). Record in a DEC the margin and that the Hub's 4 wrong
      above-floor proposals are gone while `Obsidian Publish. → Obsidian Publish.md` (1.0)
      stays.
- [x] FIX-3 (G2): decide-or-implement basename fallback matching `X` to `**/x/index.md` for
      directory-index corpora (MDN's `Anchor_positioning` → `guides/anchor_positioning/
      index.md` scores 0.76 today). Implement if it is a candidate-generation change under the
      existing floor; otherwise a DEC.
- [x] FIX-4 (G1): DEC on MDN slug encoding (`:` → `_colon_`, `*` → `_star_`, `::` →
      `_doublecolon_`; 267 of MDN's 450 unresolved prefixed links). Preferred: a `[links]
      slug_map` table in `.hyalo.toml` applied before resolution, reported by `hyalo config`;
      if declined, the DEC says why and the count stays in the report.

## Part E — Hints that keep the answer stable (BUG-15, 47; UX-8, 9, 11, 13) [6/6]

- [x] HINT-1 (BUG-15): every hint threads `--site-prefix <value>` when it was given on the
      CLI, exactly as `--dir`, `--format` and `--index-file` are threaded (UX-2 of the
      previous report); the "a .hyalo-index snapshot exists" hint additionally refuses to
      suggest `--index` when the snapshot's prefix differs from the run's.
- [x] HINT-2 (BUG-47): the `find --broken-links` text hint that suggests `links fix` stays
      silent when every broken link is site-absolute (iteration 274 listed this as shipped;
      MDN without a prefix still prints it).
- [x] HINT-3 (UX-9): `hyalo config` reports a derived `site_prefix` with
      `site_prefix_source: "derived"` **and** a note that it came from the directory name;
      `links fix` says "derived from the directory name" in its stripped-0-of-N warning so
      the MDN `en-us` trap explains itself.
- [x] HINT-4 (UX-13): `find --index --file <missing>` reports `file not found` without the
      stale-index warning first.
- [x] HINT-5 (UX-8): the stale-index warning says which probe fired ("directory mtime" vs
      "file <x> changed") so the witness name appearing on one run and not the next is
      explained.
- [x] HINT-6 (UX-11): `hyalo find <existing-file.md>` (positional, no other filters) hints
      the `--file` form and `hyalo read` in one line rather than "No results".

## Part F — Read-side UX (UX-6, 10, 12; G6) [4/4]

- [x] READ-1 (UX-6): `find --broken-links --format text` prints only the broken links of each
      file (JSON already carries `broken_anchor`/`path: null`; no shape change).
- [x] READ-2 (UX-10): `<https://…>` and `<obsidian://…>` autolinks are inventoried as
      `external` so external-target histograms are complete.
- [x] READ-3 (UX-12): `--property 'k!=v'` semantics on an absent key are documented in
      `find --help` next to the sequence case (GitHub Docs: 1249 files lack `versions.fpt`).
- [x] READ-4 (G6): a documented `--jq` recipe for a link-kind histogram and for listing
      missing images in `skill-hyalo.md` / `.claude/CLAUDE.md`, validated by
      `check-jq-recipes`; no `--links-kind` flag.

## Shared closing tasks [5/5]

- [x] Changelog entries via `hyalo changelog add` (one per part, listing the items).
- [x] DECs in [[decision-log]]: edge definition (GRAPH-1), fuzzy runner-up margin (FIX-2),
      basename/index fallback (FIX-3), slug map (FIX-4); DEC-280 amended for `skipped` in the
      header.
- [x] Help texts, `rule-knowledgebase.md`, `skill-hyalo.md`, `.claude/CLAUDE.md` updated in
      the same PR; the performance numbers (before/after, Hub and MDN) recorded in the
      Outcome and in `research/` if a perf note exists for the previous batch.
- [x] Every unfinished item moved to `backlog/` with its repro.
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, every xtask `check-*`
      gate, plus `bench-scale` run once locally with the numbers in the Outcome.

## Acceptance criteria and dispositions

- **Partially verified:** Hub copy: `lint --fix` applied ≤ 10 s (was 49 s),
      a 2190-backlink `mv` ≤ 5 s (was
      25 s), results byte-identical to the serial path; a progress line appears past a few
      hundred writes.
      `lint --fix` measured 2.35 s; the 2190-backlink timing case was not reproduced.
      [[iterations/iteration-278-site-prefix-resolution-allocation]] subsequently added a
      repeatable 2000-backlink benchmark (0.52 s), not a verification of the original fixture.
- **Targets missed here; followed up in iteration 278:** MDN with
      `--site-prefix en-US/docs`: indexed `summary` ≤ 0.8 s, `find --broken-links
      --count` ≤ 0.6 s, `create-index` ≤ 4 s; disk/index parity still byte-identical.
      This iteration measured 2.42 s, 1.71 s and 2.59 s respectively, with parity intact.
      [[iterations/iteration-278-site-prefix-resolution-allocation]] records later indexed
      measurements (0.26 s summary, 0.30 s find); the find measurement uses `--index`, unlike
      the original command above.
- [x] `summary.orphans` equals `find --orphan --count` and `summary.dead_ends` equals
      `find --dead-end --count` on MDN and the Hub; `summary --index` reports `skipped`.
- **Partially met here; scorer follow-up completed:** Every fuzzy plan carries
      `emitted_target`; the Hub's `Cat`, `jamesb`, `paulbricman`
      and `obsidian-floating-toc-plugin` proposals fall below the floor; `broken_anchors`
      matches `find`.
      Reporting and `jamesb` were fixed here. The other three proposals were resolved in
      [[iterations/iteration-279-fuzzy-scorer-near-neighbor-stems]], whose Outcome records
      the measured scores; they were not closed by this iteration's runner-up margin.
- [x] Following the `--index` hint printed by `find --broken-links --site-prefix …` on MDN
      yields the same count as the command that printed it.
- [x] Gates green; changelog; DECs.

## Outcome

Shipped as PR against `iter-277/link-graph-parity-and-write-performance`. Five DECs
(317–321), 16 changelog entries, 14 new e2e tests in
`crates/hyalo-cli/tests/e2e/iteration277_graph_parity_and_write_perf.rs`.

### Performance — measured, before and after

`BEFORE` is `origin/main` built into a worktree; `AFTER` is this branch. Both release
builds, same machine (macOS / APFS), corpora copied fresh for each write run.

**Write phase (Obsidian Hub, 6 437 files)**

| operation | before | after | target |
|---|---|---|---|
| `lint --fix` | 48.11 s | **2.35 s** | ≤ 10 s |

The 2190-backlink `mv` could not be reproduced as a timing case: the Hub's most-linked note
completes in 0.03 s on *both* binaries, because a single-file `mv` rewrites only the files
that link to it and that set is small enough (≤ 8 files in the sampled case) to stay on the
durable path. The `lint --fix` figure is the honest measurement of the same cost.

Root cause, established before the fix (PERF-1): `File::sync_all` on macOS is
`fcntl(F_FULLFSYNC)` at **5.5 ms per file regardless of size**, and it does **not** amortize
across threads — 5.25 ms/file on one thread, 4.08 ms on four, 4.78 ms on sixteen. Parallelising
the rewrite phase, which this iteration also did, could never have reached the target alone;
DEC-317 (one durability fsync per touched directory, atomicity untouched) is what did.

**Site-prefix resolution (MDN, `files/en-us`, 14 375 files, `--site-prefix en-US/docs`)**

| operation | before | after | target |
|---|---|---|---|
| `summary --index` | 4.47 s | **2.42 s** | ≤ 0.8 s |
| `find --broken-links --count` | 4.75 s | **1.71 s** | ≤ 0.6 s |
| `create-index` | 3.08 s | **2.59 s** | ≤ 4 s |

Roughly halved, but the two aggressive targets were **not** met. Two rounds were needed: the
first (exact-path membership only) bought 11 %, because MDN writes `/en-US/docs/Web/CSS/...`
against lowercase directories, so almost every link missed the exact-path check and fell
through to the filesystem anyway. Resolving the *case-folded* hit from the index too — which
returns the identical canonical path the disk probe would have — is what produced the numbers
above. What remains is not filesystem work: it is the per-link allocation in `resolve_target`
(`replace`, `strip_site_prefix`, `to_ascii_lowercase` per probe, `format!` for the `.md` and
`/index.md` candidates) across ~51 000 links and three probes each. Reducing that means a
borrow-based resolution path, which is its own iteration — filed as a follow-up rather than
rushed in beside a write-safety change.

**Parity held throughout.** On MDN, disk and `--index` report identical
`find --broken-links` totals (1 370) and identical `summary` (orphans 3 446, dead ends 854,
files 14 375 with the same per-directory breakdown).

### Graph parity

`summary.orphans` == `find --orphan --count` == **3 446** and `summary.dead_ends` ==
`find --dead-end --count` == **854** on MDN, through the one predicate DEC-318 introduced
(`types::is_note_graph_edge`). The root cause was not what the plan guessed: the link graph
*did* exclude attachments, but by asking its own case index — which holds notes only — whether
the target resolved to one. The exclusion therefore fired for `find` and never for `summary`.
A predicate needing neither index nor filesystem cannot drift between call sites at all.

### `links fix` reporting — what DEC-319 reaches, and what it does not

Every `fuzzy_fixes` entry now carries `emitted_target` (BUG-17), `broken_anchors` is the count
`find` computes rather than a constant 0 (BUG-45), and the two `site_prefix` warnings count
one shared set (BUG-46).

The runner-up margin (DEC-319) does **not** close all four proposals the acceptance criteria
named. Measured on a Hub copy: `[[jamesb]]` → `jamesgreenblue.md` fell from 0.885 to 0.337
and is now below the floor, but `[[Cat]]` → `CatMuse.md` (0.867), `[[paulbricman]]` →
`paultreanor.md` (0.855) and `[[obsidian-floating-toc-plugin]]` → `obsidian-plugin-toc.md`
(0.857) did not — raising the margin to 0.10 moved none of them, so their runner-ups are far
away and they are not near-ties. The plan's premise ("the runner-up scores within a margin")
holds for one of the four; the other three are a *scorer* problem — one similar-stem candidate
rated too highly on its own — and widening the ambiguity margin far enough to catch them would
damp genuinely unique matches. Recorded in DEC-319 and left for a scorer iteration rather than
tuned until the four examples happened to pass.

### Review fix: the shared predicate over-generalised the extension rule

`/review-pr` caught a regression in DEC-318's `is_note_graph_edge` before merge, verified
empirically against an `origin/main` build: a note whose own stem contains a dotted suffix
(`Foo.v2.md`, linked as `[[Foo.v2]]`) resolves correctly, but the predicate judged
"attachment" purely from the target's spelling (`v2` looks like an extension), so
`backlinks Foo.v2.md` came back empty and `find --orphan` / `summary.orphans` called it an
orphan despite a real inbound link — the exact case the predicate's own doc comment warned
against, now real. Fixed by making `is_note_graph_edge` take the attachment verdict as a
caller-supplied fact rather than recomputing it from syntax: `link_graph.rs` restores an
index-aware `target_is_attachment` (falling back to the syntactic guess only when the index
has no opinion, which still correctly excludes a broken `![[missing.png]]`), and `find/mod.rs`
trusts the link's already-resolved `path` when present. A regression test
(`a_dotted_note_stem_stays_a_graph_edge`) pins it. Also fixed in the same pass: a stray run of
~30 literal spaces in the "index older than vault" warning string (a missing
backslash-continuation). Full workspace suite green afterward (2122 e2e).

### Deferred

- **GRAPH-4** (`ambiguous` + `candidates` on every link record) →
  [[backlog/link-ambiguity-as-a-first-class-link-field]]. The reporting half is small; the
  plan's real requirement — that `mv`, `backlinks` and HYALO006 read the same field — is a
  cross-module convergence of the same shape DEC-318 turned out to be, and shipping only the
  reporting half would have added a *fourth* place that computes ambiguity.
- **FIX-3** and **FIX-4** closed as won't-do (DEC-321, DEC-320), which the plan named as
  acceptable outcomes for both.

### Gates

`cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace -q`, and every xtask `check-*` gate
(`check-feature-fanout`, `check-help-drift`, `check-command-reference`,
`check-bundled-skills`, `check-pi-package-sync`, `check-jq-recipes` — 42 recipes,
`check-mutation-journal`) all green.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-271-274]] — BUG-13, 14, 15, 16, 17, 18, 24, 45, 46, 47; UX-6, 8, 9, 10, 11, 12, 13; G1, G2, G3, G6
- [[iterations/iteration-271-write-and-rewrite-safety]] — the atomic-write path, `classify_link`
- [[iterations/iteration-272-resolution-completeness]] — `emitted_target`, DEC-297
- [[iterations/iteration-274-hints-help-and-contract-polish]] — hint threading, UX-3 volume gate
- [[decision-log]] — DEC-098, DEC-275, DEC-280, DEC-286, DEC-295, DEC-297
