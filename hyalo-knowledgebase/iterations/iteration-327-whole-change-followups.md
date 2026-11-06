---
title: "Iteration 327: whole-change follow-ups"
type: iteration
date: 2026-11-06
tags: [iteration, review, docs, index]
status: completed
branch: iter-327/whole-change-followups
priority: 1
related: ["[[decision-log]]", "[[backlog/link-inventory-ignores-protected-regions]]", "[[backlog/regression-dogfood-2026-10-05-leftovers]]"]
---

# Iteration 327: whole-change follow-ups

A read of everything since v0.25.1 as one change found places where pull requests disagree
with each other, or the docs with the code. This iteration fixes them so the next release
says what it does. Decisions: DEC-399 (behaviour), DEC-400 (text only).

## Tasks

### Upgrade notes and docs

- [x] W1: `changelog add --help` no longer says "exits non-zero on drift"; user-facing changes listed under `**Breaking:**` in the CHANGELOG (dry-run exit codes, severity refusal, `init`/`deinit`, `new --file`, the snapshot rebuild)
- [x] W4: one CHANGELOG entry for the snapshot format, naming the final one
- [x] W5: stale docs updated (`rule-knowledgebase.md`, `.claude/CLAUDE.md`, `docs/configuration.md`), pointing at `hyalo config --jq .results.snapshot_format_version`
- [x] W6: the CHANGELOG lists the public API diff of `hyalo-core` against 0.25.1
- [x] W12: the `%%` block change and the `types`/`new` exit 1 are in the CHANGELOG
- [x] W13: DEC-363 and DEC-390 say they cover file selection, not link targets, and link the backlog item

### Behaviour

- [x] W8: a partial path two notes end in is ambiguous on every reader, on disk and under `--index`
- [x] W10: `new` and `types` work under an invalid lint severity
- [x] W14: `deinit` removes only the exact config `init` wrote
- [x] W7: `autofixable` follows the findings; no `--fix` hint for what `--fix` will not change
- [x] W9: one snapshot refusal for the loader, the hints and `hyalo config`
- [x] W11: `links fix` reports `emitted_target` per occurrence and its hint counts every bucket
- [x] W2: a snapshot is refused over `[scan] include` and ignore files, format 11

### Decided, text only

- [x] W3: DEC-381 states what the protected-region model governs; backlog item filed

### Proof

- [x] Baseline differential on MDN, GitHub Docs, Obsidian Hub, hints off
- [x] Disk against `--index`, byte-compared, outside the documented provenance keys
- [x] Incremental `create-index` against `--force`; W2 test for an edited ignore file and `[scan] include`
- [x] MDN timing, baseline against new
- [x] W8 table test across `find`, HYALO006, `links fix`, `mv`, `summary`, on disk and `--index`
- [x] Gates: fmt, clippy, tests, every xtask gate, npm gate, case-sensitive e2e, `hyalo lint --strict`

## Notes

W2 costs one `stat` per recorded ignore source at load. Not detected: a nested ignore file
created after the build, an edit within the build's own second.
