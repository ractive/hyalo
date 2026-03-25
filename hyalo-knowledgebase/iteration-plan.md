---
date: 2026-03-20
status: active
tags:
- plan
- iteration
title: Hyalo — High-Level Iteration Plan
type: plan
---

# High-Level Iteration Plan

## Iteration 1 — Frontmatter Parser & Property Commands

The foundation. Parse YAML frontmatter, infer types, implement property commands. After this iteration, hyalo will be used to manage its own knowledgebase (dogfooding).

**Commands:** `properties`, `property:read`, `property:set`, `property:remove`

## Iteration 2 — Wikilink Parser & Link Commands

Parse `[[wikilinks]]`, `![[embeds]]`, and `[markdown](links)`. Custom streaming scanner for line-by-line processing. Simple direct link resolution via filesystem probes.

**Commands:** `links` (with `--resolved`/`--unresolved` filter flags)

**Deferred to Indexing:** `backlinks`, `orphans`, `deadends` (require full vault scan per call)

## Iteration 3 — Tags & Tasks

Parse inline `#tags` (including nested) and task checkboxes with any status character.

**Commands:** `tags`, `tag`, `tasks`, `task` (with toggle/status)

## Iteration 4 — Property Find & List Operations

`property find` for searching files by frontmatter values. Generic list-property mutations (`add-to-list`, `remove-from-list`). Refactored `tag add/remove` to delegate to generic list ops.

**Commands:** `property find`, `property add-to-list`, `property remove-from-list`

## Iteration 5 — Summary + List Subcommand Refactor

Split `properties` and `tags` into `summary` (aggregate, default) and `list` (per-file detail) subcommands. Extracted shared helpers, fixed clippy pedantic warnings.

**Commands:** `properties summary|list`, `tags summary|list`

## Iteration 6 — Outline Command + Typed Structs

Section-aware structural extraction for LLM navigation. Headings with line numbers, frontmatter properties with types and values, tags, wikilinks per section, task counts per section, code block languages per section. Supports single-file, glob, and vault-wide modes.

Also introduced `src/types.rs` with `#[derive(Serialize)]` structs for all JSON output shapes, refactoring all existing commands to use typed structs instead of ad-hoc `json!()` macros (DEC-025).

**Commands:** `outline`

## Iteration 7 — Human-Readable Text Output via jaq

Replaced the generic key=value text formatter with proper human-readable output for all commands. Each output type gets a jq filter string executed via the `jaq` crate (pure Rust). Filter lookup is based on sorted top-level JSON keys. Unknown shapes fall back to old generic format. (DEC-027)

**No new commands** — purely output layer change. See [[iterations/done/iteration-07-text-output]].

## Iteration 8 — Task Commands (superseded)

Original plan for task commands. Superseded by iteration 9 which combined tasks with summary and a unified scanner.

## Iteration 9 — Task Commands + Summary + Unified Scanner

Combined task commands (`task read`, `task toggle`, `task set-status`) with a `summary` command for vault-wide overview. Introduced the multi-visitor scanner architecture (DEC-028, DEC-029).

**Commands:** `tasks`, `task read|toggle|set-status`, `summary`

## Iteration 10 — Comment Block Handling

Scanner now skips `%%comment%%` blocks, preventing false positives for wikilinks and tasks inside comments.

## Iteration 11 — Discoverable Drill-Down Commands

Added `--hints` / `--no-hints` flags and `.hyalo.toml` support. Output includes copy-pasteable follow-up commands (DEC-031).

## Iteration 12 — CLI Redesign: find/set/remove

Major breaking change. Replaced the many subcommands with a unified `find` query + `set`/`remove`/`append` mutations. `find` absorbs property search, tag search, task filtering, content search, and structural extraction into one command. (See research/unified-find-command.md)

**Commands:** `find`, `set`, `remove`, `append`, `properties`, `tags`, `summary`, `task`

## Iteration 13 — Read Command (planned, not yet implemented)

Display file content from the CLI. Still planned but skipped in favour of iter-14+.

## Iteration 14 — Text Output Overhaul

Rewrote `--format text` rendering: quoted paths, group labels, newlines between entries, suppressed empty sections.

## Iteration 15 — Performance Benchmark Suite

Added Criterion.rs micro-benchmarks and Hyperfine CLI benchmarks. Established baseline performance numbers.

## Iteration 16 — Robustness

Hardened malformed-file handling and path edge cases. Better error messages for broken frontmatter, UTF-8 issues, path traversal.

## Iteration 17 — Per-Project Config File (.hyalo.toml)

Added `.hyalo.toml` config for `--dir`, `--format`, `--hints` defaults. CLI flags always override.

## Iteration 18 — Parallel Processing (shelved)

Experimented with rayon `par_iter()`. Benchmarks showed no meaningful improvement on SSD — I/O is not the bottleneck. Shelved until vault size justifies it. See research/performance-parallelization.md.

## Future — Indexing

SQLite or similar index for properties, tags, and links. Incremental updates based on file mtime. Triggered when file scanning becomes a bottleneck on large vaults.

**Deferred commands from iteration 2:** `backlinks`, `orphans`, `deadends` — these require full vault scans and benefit most from indexing.

**Deferred from iteration 2:** Obsidian shortest-path resolution (`[[foo]]` matching `sub/foo.md`). Currently link resolution requires explicit paths; shortest-path lookup can be added once an index exists.

## Future — Read Command

Display file content (or sections) from the CLI. Originally planned as iteration 13.

## Future — Move/Rename with Link Updates

Move or rename a file and update all wikilinks across the knowledge base. Originally planned as iteration 9.

## Dependencies

```
Iteration 1 (frontmatter) ──→ Iteration 4 (find needs parser)
Iteration 2 (link graph)  ──→ Iteration 6 (outline reuses scanner)
Iterations 1–7             ──→ Iteration 12 (find unifies all queries)
```

## Dogfooding

Starting after iteration 1, hyalo manages its own `hyalo-knowledgebase/`.
