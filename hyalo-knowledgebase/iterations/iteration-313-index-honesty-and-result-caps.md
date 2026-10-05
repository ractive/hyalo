---
title: "Iteration 313: index honesty and result caps"
type: iteration
date: 2026-10-04
tags: [iteration, index, snapshot, hints, recipes, dogfooding]
status: completed
branch: iter-313/index-honesty-and-result-caps
priority: 2
related:
  - "[[dogfood-results/dogfood-v0250-pre-search-roadmap-2026-10-04]]"
---

# Index honesty and result caps

## Problem

[[dogfood-results/dogfood-v0250-pre-search-roadmap-2026-10-04]] found that the old-snapshot refusal warning is silenced by `-q` (BUG-7), that a
`[search] code_blocks` mismatch between snapshot and config falls back to disk
silently and cannot be discovered from hyalo (BUG-8), that `summary --index` on a
refused snapshot reports `index_format_version: null`, that a no-op `create-index`
still rewrites the whole snapshot and `--force` is slower than deleting it, and that
the 50-item caps of `find` and `lint` are invisible to scripts and the shipped
recipes (UX-5). DEC numbers reserved for this iteration: **DEC-359 to DEC-361**.

## Tasks

- [x] BUG-7: the "index format is older than this binary" refusal goes through the `-q`-proof warning facility (iteration 309 contract), like the missing-index fallback
- [x] BUG-8: a snapshot built under a different `[search] code_blocks` than the config is refused with a `-q`-proof warning naming both values; `summary --index` exposes the snapshot's `code_blocks` and, on a refused snapshot, its `index_format_version` and a `source: "disk"` style field so the fallback is visible in JSON
- [x] A no-op `create-index` (every entry reused, nothing removed) keeps the existing snapshot instead of rewriting it and says so in `results`; `--force` does not load the old snapshot before discarding it
- [x] UX-5: `find` JSON carries `truncated: true` whenever `--limit` cut the result list (`total` already exceeds `results` length); the lint JSON `files_truncated` hint names `-n 0`
- [x] UX-5: every shipped `--jq` recipe that walks `.results[]` (`.claude/CLAUDE.md`, `skill-hyalo.md`, `rule-knowledgebase.md`, `pi-package/`, README if any) passes `--limit 0`; `check-jq-recipes` fails on a `.results[]` recipe without it
- [x] `summary` hints "refresh the stale index" rather than "create an index" when `.hyalo-index` exists and drifted
- [x] `terms`: decide and record whether joined identifier wholes (`workflowsyntaxforgithubact`) are listed by `terms` and counted against the 256-term prefix cap (DEC)
- [x] Docs in sync: `create-index --help`, `summary --help`, `.claude/CLAUDE.md` claims paragraph, `skill-hyalo.md`, CHANGELOG `[Unreleased]`, decision-log entries

## Acceptance criteria

- [x] On the real MDN tree with its v2 snapshot, `hyalo find --index -q closures` prints the refusal warning on stderr
- [x] Index built under `code_blocks = "index"`, config switched to `"skip"`: `find --index` warns under `-q` and `summary --index` JSON shows both values
- [x] MDN no-op `create-index` finishes well under the 1.8 s measured and leaves the snapshot bytes unchanged; `--force` is not slower than a fresh build
- [x] `hyalo find --broken-links --format json` on GitHub Docs carries `truncated: true` at the default limit and no `truncated` key with `--limit 0`; `check-jq-recipes` passes
- [x] Exit codes 0/1/2 (DEC-307); fmt, clippy `-D warnings`, `cargo test --workspace -q`, `cargo deny check`, help-drift and `hyalo lint --strict` green; CI green on three platforms

## Outcome

Implemented and verified against the real read-only MDN tree (`/Users/james/devel/mdn/files/en-us`,
14 375 notes, v2 `.hyalo-index`) and a scratch copy at `target/scratch/mdn-313/` for anything
writing an index, plus GitHub Docs (`/Users/james/devel/docs/content`) for the `truncated`
field.

**BUG-7.** `hyalo --dir /Users/james/devel/mdn/files/en-us find --index -q closures` prints
`warning: index format is older than this binary (index v2, binary v6); falling back to disk
scan — re-run create-index` on stderr even with `-q` (it was `crate::warn::warn`, now
`crate::warn::warn_always`).

**BUG-8.** On the MDN scratch copy, built fresh under `[search] code_blocks = "index"`, then
queried with the config switched to `"skip"`: `find --index -q` warns `index was built with
[search] code_blocks = "index", this run's config says "skip"; falling back to disk scan` and
actually serves the disk-under-"skip" answer (the fenced word dropped out). `summary --index`
on the same refused snapshot reports `index_format_version: 6`, `code_blocks: "index"`,
`source: "disk"` — previously `index_format_version: null` with no `code_blocks`/`source` key
at all. On a successful `--index` run, `summary` reports `code_blocks: "index"`,
`source: "index"`.

**No-op `create-index` / `--force`.** MDN scratch copy, 14 361 files, release build:
- Fresh build: 4.07-5.73 s (machine load varied across runs).
- No-op rerun (nothing changed): **1.69 s**, `written: false`, snapshot bytes and mtime
  byte-identical before/after (md5 compared).
- A real change (one new file): `written: true`, `reused: 14361`, `refreshed: 1`.
- `--force`: 4.07-4.14 s across three runs; a fresh build after `rm .hyalo-index`: 3.63-4.09 s
  across three runs — within the same noise band, not reliably one-sided either way. Code
  inspection confirms `--force` already short-circuits to `previous = None` before any load
  (`force || !replacing_existing`); the dogfood's single-run 4.07 s vs 3.12 s gap does not
  reproduce as a systematic difference here.

**UX-5 `truncated`.** GitHub Docs, `find --broken-links --format json`: default limit —
`total: 1904`, `results` length 50, `truncated: true`; `--limit 0` — `results` length 1904,
no `truncated` key at all (omitted, never `false`, matching the AC's exact wording). Every
shipped `.results[]`-walking `--jq` recipe in `.claude/CLAUDE.md`, `skill-hyalo.md` and
`rule-knowledgebase.md` now carries `--limit 0`; the one deliberate `--limit 5` preview recipe
in `skill-hyalo.md` is left alone (an explicit limit is not a silent cap). `check-jq-recipes`
gained a check for this (`reads_results_array_without_limit_flag`) and passes (47 recipes
executed, 1 skipped as not exercisable in this vault).

**Stale-index hint wording.** Verified on the MDN scratch copy: touching a file's own mtime
(no directory mtime change) does **not** flip the hint — the probe is deliberately cheap
(directory mtimes only, like DEC-280's first pass) and documented as missing pure content
edits. Adding a new file (moves the directory's mtime) does: `summary` on a drifted vault
hints "the on-disk `.hyalo-index` snapshot looks stale; refresh the stale index for faster
queries" → `create-index`. On a fresh, unmodified snapshot it hints "a `.hyalo-index` snapshot
exists; re-run with --index for faster queries" → `summary --index` (this half was a bonus:
the pre-iter-313 code did not consult `snapshot_on_disk` for this specific hint at all, so it
always said "create an index" even when a current one already existed).

**`terms` / prefix-cap joined wholes (DEC-359).** Decided **not** to filter this iteration —
recorded with the reasoning (no per-term provenance exists without another snapshot-format
bump, and this iteration's bug list doesn't need one) rather than implemented. Matching
behaviour is unaffected either way, which was the actual constraint.

**Known pre-existing issue, not caused by this iteration:** the e2e test
`index_incremental::racily_clean_entry_is_rescanned_not_reused` failed once during this work
(timing-sensitive — the test deliberately races a same-second rewrite). Reproduced identically
on a clean checkout of `origin/main` before any of this iteration's changes, with the same
assertion failure; it passed on every subsequent rerun on this branch (ran the full
`cargo test --workspace -q` suite four times total during this iteration, three clean). Not
investigated further — out of scope for BUG-7/8/create-index/UX-5.

**DEC numbers used:** DEC-359 (terms/prefix-cap joined wholes, won't-filter), DEC-360
(`code_blocks` mismatch = full snapshot refusal, like an old format version; BUG-7's
`warn_always` fix recorded in the same entry), DEC-361 (no-op `create-index` skips the write;
`--force` never-loads-first confirmed, not changed).

**Rebase onto main (PR #378 / iteration 314 landed mid-flight).** Iteration 314 merged while
this branch was in flight, bumping `SNAPSHOT_FORMAT_VERSION` 5 → 6 (DEC-365,
`IndexEntry.valid_utf8`) and appending DEC-362..365 to `decision-log.md`/`CHANGELOG.md`/the
`.claude/CLAUDE.md` claims paragraph. Rebased onto `origin/main`; `run.rs`, `args.rs` and the
two templates auto-merged cleanly, three files needed manual conflict resolution
(`.claude/CLAUDE.md`, `CHANGELOG.md`, `decision-log.md`) to keep both sides' additions with
DECs in numeric order (359-361 before 362-365, per the coordinator's instruction, even though
362-365 landed on main first). No snapshot-format collision — this iteration never touched
`IndexEntry`'s shape, so v6 stands as-is; the measured numbers above were re-verified against
the rebased build on the real MDN tree (BUG-7's "index v2, binary v6" and BUG-8/no-op/
`summary --index` all re-confirmed) and GitHub Docs (`truncated`) after the rebase.

**Gates:** `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace -q` (2499 e2e + 1123 lib + 1688 doctest-adjacent after the rebase, all
passing), `cargo deny check`, `check-help-drift`, `check-jq-recipes`, `check-ts-types`
(regenerated `Envelope`/`MutationReportEnvelope`/`SearchEnvelope`/`VaultSummary` TS
declarations and the `pi-package`/`crates/hyalo-cli/templates/pi` `hyalo-api.d.ts` vendored
copy via `sync-pi-package`), `check-pi-package-sync`, `check-pi-runtime`, `check-typed-output`,
and `hyalo lint --strict` (exit 0) all green, re-run and still green after the rebase. CI
across three platforms is the one box left
unticked, per instructions — that's the PR's job.
