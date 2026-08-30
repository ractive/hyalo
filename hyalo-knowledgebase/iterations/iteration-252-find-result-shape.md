---
type: iteration
title: "Iteration 252 — find result shape: compact default fields and size metadata"
date: 2026-08-29
status: completed
tags:
  - iteration
  - agent-cli
  - breaking
branch: iter-252/find-result-shape
depends-on: "[[iterations/iteration-251-agent-discoverability-help]]"
---

# Iteration 252 — `find` result shape: compact default fields and size metadata

## Goal

Measured 2026-08-29 on the own KB: `hyalo find --tag iteration --limit 20
--format json` is **73 KB**; the same with `--fields properties` is 8 KB.
The default field set (`properties, tags, sections, links`) makes every
listing pay ~9× for structure nobody asked for, and nothing in a result
tells an agent how big a file is before it `read`s it. Source: axi.md
principles review ("minimal default schema", "truncation with size
hints"). This is the **only breaking change** in the 250–252 batch, so it
gets its own iteration and a minor bump.

**No new flags.** `--fields` exists; the auto-include precedent exists
(`--broken-links` adds `links`, `--orphan` adds `backlinks`).

## Tasks [7/7]

- [x] New `find` default field set: `file, modified, size, title,
      properties, tags` (+ `score`/`matches` when a PATTERN or `-e` is
      present, as today). `sections`, `links`, `tasks`, `backlinks`,
      `properties-typed` only on request (`--fields …` or `all`) or via
      the existing auto-includes (`--section` → `sections`, `--task` →
      `tasks`, `--broken-links`/`--dead-end` → `links`, `--orphan` →
      `backlinks`, `--sort links_count|backlinks_count` → the counted field).
- [x] Size metadata: `size` (bytes) and `lines` on every `find` item and
      on `read` results; `read` emits a hint (`--lines 1:80`, `--section`)
      when the body exceeds a threshold (~8 KB). Text mode shows size in
      the header line.
- [x] Every `find` result set carries one hint: `--fields all` (or the
      specific missing field when a filter implies it). Text summary line
      names the fields included.
- [x] Snapshot index: `size`/`lines` stored per entry; `create-index`
      populates them; mutation journal keeps them current; index/disk
      parity tests extended (the iter-243/244 parity suite).
- [x] Views: a saved view may pin `fields`; views without it get the new
      default. Document in `views --help`.
- [x] Sweep consumers of the old default: hints generators, bundled skills,
      `.claude/rules`, README examples, cookbook `--jq` recipes that read
      `.results[].sections` / `.links` — add `--fields` where needed.
- [x] CHANGELOG `[Unreleased]` → **Changed** with a migration line
      (`--fields all` restores the previous shape); bump workspace version
      to 0.22.0 in the three Cargo.toml spots (minor: breaking default).
- [x] e2e: default shape byte-budget test (20-file listing ≤ 12 KB on the
      test vault), auto-include matrix, `size`/`lines` correctness on CRLF
      and UTF-8 files, index parity for the new fields.

## Acceptance criteria [4/4]

- [x] `find --tag iteration --limit 20 --format json` on the own KB ≤ 12 KB;
      `--fields all` reproduces the pre-change shape.
- [x] Every filter that implies a field still returns it without
      `--fields`.
- [x] `size` and `lines` present on `find` and `read` items, identical
      between disk scan and `--index`.
- [x] Gates and CI green; version 0.22.0.

## Outcome (2026-08-30)

Measured on the own KB after the change: `find --tag iteration --limit 20
--format json` is **11.9 KB** (was 73 KB); `--fields all` is 231 KB, i.e. the
old shape and then some. Default items carry `file, modified, size, lines,
title, properties, tags`.

Two deliberate departures from the plan, both recorded in
[[decision-log#DEC-252]]:

- **`title` is promoted out of `properties`.** Adding `title` to the default
  set made the duplicate copy inside `properties` visible — the same string
  twice per item, and the 1.4 KB that separated 13.3 KB from the 12 KB
  acceptance criterion. Removed after sorting/filtering, so `--sort
  property:title` and `--property title=…` are unaffected; `--fields
  properties` alone still carries the property. Breaking for
  `.results[].properties.title` readers; swept and changelogged.
- **The `--fields all` hint is not on every result set.** It fires for an
  untruncated set of ≤5 items. `--fields all` is the ~10x payload this
  iteration removed, and at `MAX_HINTS = 5` an unconditional hint displaced
  the narrowing hints that matter on a large listing. The always-on statement
  moved to the `--format text` `fields:` summary line and `find --help`.

Also landed beyond the plan's letter: `--sort links_count` /
`--sort backlinks_count` now *return* the field they rank on (the plan listed
this as an existing auto-include; it was in fact stripped after sorting), and
`scanner::scan_file_multi_stats` counts lines without giving up the
frontmatter-only fast path's 16 KiB prefix read for the scan itself.

## Non-goals

- Truncating `read` bodies (agents need the full text; hints suffice).
- Changing text-mode layout beyond the size column and summary line.

## Links

- [[iterations/iteration-251-agent-discoverability-help]]
- [[iterations/iteration-244-index-remaining-deferrals]]
