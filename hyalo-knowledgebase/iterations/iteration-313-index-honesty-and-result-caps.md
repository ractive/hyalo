---
title: "Iteration 313: index honesty and result caps"
type: iteration
date: 2026-10-04
tags: [iteration, index, snapshot, hints, recipes, dogfooding]
status: planned
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

- [ ] BUG-7: the "index format is older than this binary" refusal goes through the `-q`-proof warning facility (iteration 309 contract), like the missing-index fallback
- [ ] BUG-8: a snapshot built under a different `[search] code_blocks` than the config is refused with a `-q`-proof warning naming both values; `summary --index` exposes the snapshot's `code_blocks` and, on a refused snapshot, its `index_format_version` and a `source: "disk"` style field so the fallback is visible in JSON
- [ ] A no-op `create-index` (every entry reused, nothing removed) keeps the existing snapshot instead of rewriting it and says so in `results`; `--force` does not load the old snapshot before discarding it
- [ ] UX-5: `find` JSON carries `truncated: true` whenever `--limit` cut the result list (`total` already exceeds `results` length); the lint JSON `files_truncated` hint names `-n 0`
- [ ] UX-5: every shipped `--jq` recipe that walks `.results[]` (`.claude/CLAUDE.md`, `skill-hyalo.md`, `rule-knowledgebase.md`, `pi-package/`, README if any) passes `--limit 0`; `check-jq-recipes` fails on a `.results[]` recipe without it
- [ ] `summary` hints "refresh the stale index" rather than "create an index" when `.hyalo-index` exists and drifted
- [ ] `terms`: decide and record whether joined identifier wholes (`workflowsyntaxforgithubact`) are listed by `terms` and counted against the 256-term prefix cap (DEC)
- [ ] Docs in sync: `create-index --help`, `summary --help`, `.claude/CLAUDE.md` claims paragraph, `skill-hyalo.md`, CHANGELOG `[Unreleased]`, decision-log entries

## Acceptance criteria

- [ ] On the real MDN tree with its v2 snapshot, `hyalo find --index -q closures` prints the refusal warning on stderr
- [ ] Index built under `code_blocks = "index"`, config switched to `"skip"`: `find --index` warns under `-q` and `summary --index` JSON shows both values
- [ ] MDN no-op `create-index` finishes well under the 1.8 s measured and leaves the snapshot bytes unchanged; `--force` is not slower than a fresh build
- [ ] `hyalo find --broken-links --format json` on GitHub Docs carries `truncated: true` at the default limit and no `truncated` key with `--limit 0`; `check-jq-recipes` passes
- [ ] Exit codes 0/1/2 (DEC-307); fmt, clippy `-D warnings`, `cargo test --workspace -q`, `cargo deny check`, help-drift and `hyalo lint --strict` green; CI green on three platforms
