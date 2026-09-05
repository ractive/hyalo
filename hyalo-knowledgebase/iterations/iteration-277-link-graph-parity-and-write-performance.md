---
title: "Iteration 277 — Link-graph parity, links fix reporting, hint threading and write performance"
type: iteration
date: 2026-09-05
tags: [iteration, links, performance, index, hints, dogfooding]
status: planned
branch: iter-277/link-graph-parity-write-perf
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

## Part A — Write performance (BUG-14)

- [ ] PERF-1: profile one `lint --fix` on a Hub copy and one 2190-backlink `mv`; confirm the
      per-file temp+fsync+rename path from iteration 271 is the cost (report: 8–11 ms per
      file, 49 s / 25 s).
- [ ] PERF-2: keep the atomicity guarantee per file but stop paying it serially: parallelise
      the rewrite phase of `lint --fix`, `mv`, `links fix --apply`, `set/append/remove --glob`
      and `properties rename`/`tags rename` with the same rayon pool the scan uses, and fsync
      once per directory at the end where the platform allows (document the Windows
      behaviour). Target: Hub `lint --fix` ≤ 10 s, the 2190-link `mv` ≤ 5 s, MDN bulk `set`
      ≥ 500 files/s.
- [ ] PERF-3: a progress line on stderr (`-q`-silenced) once a write phase passes a few
      hundred files, so 49 s of silence never reads as a hang.

## Part B — `site_prefix` resolution from memory (BUG-13, 24)

- [ ] PREFIX-1: `classify_link`/site-absolute resolution consults the in-memory file set (the
      scan's or the snapshot's) instead of `Path::is_file()`; on the snapshot path no
      filesystem access at all. Measure MDN with `--site-prefix en-US/docs`: indexed `summary`
      4.65 s → target ≤ 0.8 s, `find --broken-links --count` 2.37 s → ≤ 0.6 s, `create-index`
      6.44 s → ≤ 4 s. Re-run the parity check (byte-identical JSON) afterwards.
- [ ] PREFIX-2 (BUG-24): the snapshot header records `skipped` next to `excluded` so
      `summary --index` matches the disk scan (Hub 1, kepano 28) and keeps the skipped
      directory row.

## Part C — One graph, one answer (BUG-16, 45, 46; G3)

- [ ] GRAPH-1 (BUG-16): `summary`, `find --orphan` and `find --dead-end` share one edge
      definition. Decide in a DEC whether attachment links are edges (the report's 25 MDN
      files whose only outbound links are images); `find --help`'s "not graph edges" is the
      current promise, so the default choice is to make `summary` agree with it. Record the
      MDN numbers before and after.
- [ ] GRAPH-2 (BUG-45): `links fix` JSON `broken_anchors` reports the count `find` computes
      (MDN 10929) and is documented in `links fix --help`, or the key is removed; never a
      constant 0.
- [ ] GRAPH-3 (BUG-46): the "stripped 0 of N" and "skipped fuzzy scoring for N" warnings
      count the same set (49767 vs 49776 on MDN).
- [ ] GRAPH-4 (G3): `--fields links` records carry `ambiguous: true` with `candidates` when a
      stem or alias collides, so ambiguous and missing can be told apart without `links fix`;
      `mv`, `backlinks` and HYALO006 read the same field.

## Part D — `links fix` reporting (BUG-17, 18; G2)

- [ ] FIX-1 (BUG-17): every `fuzzy_fixes[]` entry carries `emitted_target`, computed by the
      same function as the other buckets (`links fix --help` already promises it).
- [ ] FIX-2 (BUG-18): fuzzy confidence drops below the floor when the runner-up scores within
      a margin (report: `Cat → CatMuse 0.87` with five `cat*` notes, `jamesb → jamesgreenblue
      0.885` with eight `james*`, `…/tabindex → global_attributes/index.md 0.9125` on a
      directory-index corpus). Record in a DEC the margin and that the Hub's 4 wrong
      above-floor proposals are gone while `Obsidian Publish. → Obsidian Publish.md` (1.0)
      stays.
- [ ] FIX-3 (G2): decide-or-implement basename fallback matching `X` to `**/x/index.md` for
      directory-index corpora (MDN's `Anchor_positioning` → `guides/anchor_positioning/
      index.md` scores 0.76 today). Implement if it is a candidate-generation change under the
      existing floor; otherwise a DEC.
- [ ] FIX-4 (G1): DEC on MDN slug encoding (`:` → `_colon_`, `*` → `_star_`, `::` →
      `_doublecolon_`; 267 of MDN's 450 unresolved prefixed links). Preferred: a `[links]
      slug_map` table in `.hyalo.toml` applied before resolution, reported by `hyalo config`;
      if declined, the DEC says why and the count stays in the report.

## Part E — Hints that keep the answer stable (BUG-15, 47; UX-8, 9, 11, 13)

- [ ] HINT-1 (BUG-15): every hint threads `--site-prefix <value>` when it was given on the
      CLI, exactly as `--dir`, `--format` and `--index-file` are threaded (UX-2 of the
      previous report); the "a .hyalo-index snapshot exists" hint additionally refuses to
      suggest `--index` when the snapshot's prefix differs from the run's.
- [ ] HINT-2 (BUG-47): the `find --broken-links` text hint that suggests `links fix` stays
      silent when every broken link is site-absolute (iteration 274 listed this as shipped;
      MDN without a prefix still prints it).
- [ ] HINT-3 (UX-9): `hyalo config` reports a derived `site_prefix` with
      `site_prefix_source: "derived"` **and** a note that it came from the directory name;
      `links fix` says "derived from the directory name" in its stripped-0-of-N warning so
      the MDN `en-us` trap explains itself.
- [ ] HINT-4 (UX-13): `find --index --file <missing>` reports `file not found` without the
      stale-index warning first.
- [ ] HINT-5 (UX-8): the stale-index warning says which probe fired ("directory mtime" vs
      "file <x> changed") so the witness name appearing on one run and not the next is
      explained.
- [ ] HINT-6 (UX-11): `hyalo find <existing-file.md>` (positional, no other filters) hints
      the `--file` form and `hyalo read` in one line rather than "No results".

## Part F — Read-side UX (UX-6, 10, 12; G6)

- [ ] READ-1 (UX-6): `find --broken-links --format text` prints only the broken links of each
      file (JSON already carries `broken_anchor`/`path: null`; no shape change).
- [ ] READ-2 (UX-10): `<https://…>` and `<obsidian://…>` autolinks are inventoried as
      `external` so external-target histograms are complete.
- [ ] READ-3 (UX-12): `--property 'k!=v'` semantics on an absent key are documented in
      `find --help` next to the sequence case (GitHub Docs: 1249 files lack `versions.fpt`).
- [ ] READ-4 (G6): a documented `--jq` recipe for a link-kind histogram and for listing
      missing images in `skill-hyalo.md` / `.claude/CLAUDE.md`, validated by
      `check-jq-recipes`; no `--links-kind` flag.

## Shared closing tasks

- [ ] Changelog entries via `hyalo changelog add` (one per part, listing the items).
- [ ] DECs in [[decision-log]]: edge definition (GRAPH-1), fuzzy runner-up margin (FIX-2),
      basename/index fallback (FIX-3), slug map (FIX-4); DEC-280 amended for `skipped` in the
      header.
- [ ] Help texts, `rule-knowledgebase.md`, `skill-hyalo.md`, `.claude/CLAUDE.md` updated in
      the same PR; the performance numbers (before/after, Hub and MDN) recorded in the
      Outcome and in `research/` if a perf note exists for the previous batch.
- [ ] Every unfinished item moved to `backlog/` with its repro.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, every xtask `check-*`
      gate, plus `bench-scale` run once locally with the numbers in the Outcome.

## Acceptance criteria

- [ ] Hub copy: `lint --fix` applied ≤ 10 s (was 49 s), a 2190-backlink `mv` ≤ 5 s (was
      25 s), results byte-identical to the serial path; a progress line appears past a few
      hundred writes.
- [ ] MDN with `--site-prefix en-US/docs`: indexed `summary` ≤ 0.8 s, `find --broken-links
      --count` ≤ 0.6 s, `create-index` ≤ 4 s; disk/index parity still byte-identical.
- [ ] `summary.orphans` equals `find --orphan --count` and `summary.dead_ends` equals
      `find --dead-end --count` on MDN and the Hub; `summary --index` reports `skipped`.
- [ ] Every fuzzy plan carries `emitted_target`; the Hub's `Cat`, `jamesb`, `paulbricman`
      and `obsidian-floating-toc-plugin` proposals fall below the floor; `broken_anchors`
      matches `find`.
- [ ] Following the `--index` hint printed by `find --broken-links --site-prefix …` on MDN
      yields the same count as the command that printed it.
- [ ] Gates green; changelog; DECs.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-271-274]] — BUG-13, 14, 15, 16, 17, 18, 24, 45, 46, 47; UX-6, 8, 9, 10, 11, 12, 13; G1, G2, G3, G6
- [[iterations/iteration-271-write-and-rewrite-safety]] — the atomic-write path, `classify_link`
- [[iterations/iteration-272-resolution-completeness]] — `emitted_target`, DEC-297
- [[iterations/iteration-274-hints-help-and-contract-polish]] — hint threading, UX-3 volume gate
- [[decision-log]] — DEC-098, DEC-275, DEC-280, DEC-286, DEC-295, DEC-297
