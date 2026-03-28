---
title: "Iteration 61: Link Health"
type: iteration
date: 2026-03-28
tags:
  - iteration
  - links
  - summary
  - find
status: completed
branch: iter-61/link-health
---

## Goal

Add vault-wide broken link detection and auto-repair. Surface link health in `summary`, add `find --broken-links` filter, and provide `links fix` to auto-repair them.

## Motivation

Broken links silently accumulate as files are renamed, moved, or deleted outside hyalo. There is currently no way to detect them vault-wide. The `mv` command prevents breakage during renames, but nothing catches pre-existing or externally-introduced broken links.

## Design Decisions

### Broken links in `summary` — not a new command

`summary` already reports orphans, tasks, and tag/property health. Adding a `broken_links` section keeps the health dashboard in one place rather than creating a separate `links check` command.

**New `links` section in VaultSummary:**
```json
{
  "links": {
    "total": 142,
    "broken": 3,
    "broken_links": [
      { "source": "notes/auth.md", "line": 12, "target": "[[old-middleware]]" },
      ...
    ]
  }
}
```

### `find --broken-links` — convenience filter

`find --fields links` already shows links per file with `path: None` for broken ones, but that's an implementation detail users shouldn't need to know. `--broken-links` is a dedicated boolean filter: returns only files that have at least one unresolved link, auto-includes the links field in output.

### `links fix` — new command with fuzzy matching

Auto-repair broken links by finding the best-matching existing file. Reuses `Replacement` + `apply_replacements()` from `link_rewrite.rs`.

**Match strategy (in priority order):**
1. Case-insensitive exact match (`[[Auth]]` → `auth.md`)
2. With/without `.md` extension
3. Shortest-path resolution (`[[foo]]` → `sub/deep/foo.md` if unique) — closes the deferred backlog item `shortest-path-link-resolution`
4. Edit distance (Levenshtein/Jaro-Winkler) above a configurable threshold

**New dependency:** `strsim` crate (~lightweight) for string similarity.

**CLI:**
```
hyalo links fix [--dry-run] [--apply] [--threshold 0.7] [-g/--glob PATTERN]
```
Default is `--dry-run` (preview only). Must pass `--apply` to write changes. This is a mutation command — safe defaults matter.

**Risk:** Low. New isolated module `link_fix.rs` in hyalo-core + new command file. No changes to existing link_rewrite, link_graph, or links modules.

## Risks & Concerns

1. **Index path for broken link detection.** The index already has the full file list, so broken link detection is just a set lookup — no extra stat calls needed. The disk-scan path already reads every file, so resolving links there is nearly free too. No performance concern.

2. **Fuzzy matching false positives.** `links fix --apply` could rewrite a link to the wrong target. Mitigation: `--dry-run` default, clear diff-style preview, configurable threshold.

3. **VaultSummary serialization contract.** Adding a new `links` field to VaultSummary is additive — existing JSON consumers just get an extra key. No breaking change.

## Tasks

- [x] Add `BrokenLinkSummary` and `LinkHealthSummary` types to `hyalo-core/src/types.rs`
- [x] Implement broken link detection in summary disk-scan path
- [x] Implement broken link detection in summary index path
- [x] Update summary text formatter for new links section
- [x] Add `strsim` dependency to hyalo-core
- [x] Implement `link_fix.rs` in hyalo-core (fuzzy matching + fix planning)
- [x] Implement `links fix` CLI command with `--dry-run`/`--apply`
- [x] Implement shortest-path resolution as part of fix match strategy
- [x] Add unit tests for broken link detection and fuzzy matching
- [x] Add `--broken-links` filter flag to `find` command
- [x] Add e2e tests for summary link health, find --broken-links, and links fix
- [x] Close backlog item `shortest-path-link-resolution` (subsumed by links fix)

## Out of Scope

- `links suggest` (unlinked title mentions in prose) — future iteration
- `links convert` (wikilink ↔ markdown normalization) — future iteration
- `links graph` (DOT/Mermaid export) — future iteration
- `summary --fields` selective output — separate iteration
- Caching resolution status in the snapshot index — future optimization
