---
title: "Iteration 85: Consistent JSON envelope for all commands"
type: iteration
date: 2026-03-30
status: completed
branch: iter-85/json-envelope
tags:
  - ux
  - json
  - breaking-change
  - dogfooding
---

## Goal

Unify the JSON output of all hyalo commands behind a single, stable envelope so that
`--jq` expressions never break when toggling `--hints` / `--no-hints`, and users don't
need to learn per-command shapes.

## Context

Dogfood runs on the MDN repo (14K files) revealed three problems:

1. `--hints` wraps output in `{data: ..., hints: [...]}` — changing the jq path from
   `.results[]` to `.data.results[]`.
2. `properties` is an outlier: returns a bare array, never gets hint-wrapped.
3. Different commands use different inner shapes (`{results, total}` vs flat object vs
   bare array).

See [[backlog/done/json-envelope-consistency]] for the full shape inventory.

## Design

Common envelope for **all** JSON responses:

```json
{
  "results": <command-specific payload>,
  "total": <count — present for list commands, omitted for others>,
  "hints": [{"cmd": "...", "description": "..."}]
}
```

Key decisions:
- `--jq` operates on the **full envelope** (e.g. `.results[].file`, `.total`) — not just `.results`
- `hints` array is **always present** (empty `[]` when `--no-hints`) — shape never changes
- `--no-hints` suppresses hint *generation*, not the field itself
- `total` present for: find, tags, properties, backlinks. Omitted for: summary, read,
  mutations, create-index, drop-index

## Tasks

- [x] Refactor `output.rs` envelope: replace `format_with_hints()` wrapping with a
      single `Envelope { results, total, hints }` struct used by all commands
- [x] Make `--jq` operate on the full envelope (stable shape regardless of --hints)
- [x] Wrap `properties` summary in the envelope (currently bare array — breaking change)
- [x] Wrap `summary` output in `{results: {files, orphans, ...}, hints: [...]}`
- [x] Wrap `read` output in `{results: {content, file}, hints: [...]}`
- [x] Wrap mutation outputs (set, remove, append) in envelope
- [x] Wrap `backlinks`, `mv`, `task`, `links fix`, `create-index`, `drop-index` in envelope
- [x] Ensure `--no-hints` produces `"hints": []` not absent field
- [x] Update `--help` long_about for the top-level CLI to document the envelope structure
- [x] Update `-h` short help to mention the envelope
- [x] Update `Find` command long_about: change "Returns an array of file objects" to
      describe the envelope
- [x] Update all `--jq` examples in `--help` text to use `.results` path
- [x] Update SKILL.md: document the envelope, fix all `--jq` examples
- [x] Update README.md: fix any JSON snippets and `--jq` examples
- [x] Audit knowledgebase for stale `--jq` examples (dogfood-results/, backlog/)
- [x] Update all e2e tests that assert on JSON shape
- [x] Update `text_jq_filter` tests in output.rs
- [x] Run full dogfood pass after changes to verify nothing breaks
