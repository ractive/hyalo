---
title: "Iteration 85: Consistent JSON envelope for all commands"
type: iteration
date: 2026-03-30
status: planned
branch: iter-85/json-envelope
tags:
  - ux
  - json
  - breaking-change
  - dogfood
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

See [[backlog/json-envelope-consistency]] for the full shape inventory.

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
- `--jq` always operates on `.results`, not the full envelope
- `hints` array is **always present** (empty `[]` when `--no-hints`) — shape never changes
- `--no-hints` suppresses hint *generation*, not the field itself
- `total` present for: find, tags, properties, backlinks. Omitted for: summary, read,
  mutations, create-index, drop-index

## Tasks

- [ ] Refactor `output.rs` envelope: replace `format_with_hints()` wrapping with a
      single `Envelope { results, total, hints }` struct used by all commands
- [ ] Make `--jq` operate on `.results` instead of the full envelope
- [ ] Wrap `properties` summary in the envelope (currently bare array — breaking change)
- [ ] Wrap `summary` output in `{results: {files, orphans, ...}, hints: [...]}`
- [ ] Wrap `read` output in `{results: {content, file}, hints: [...]}`
- [ ] Wrap mutation outputs (set, remove, append) in envelope
- [ ] Wrap `backlinks`, `mv`, `task`, `links fix`, `create-index`, `drop-index` in envelope
- [ ] Ensure `--no-hints` produces `"hints": []` not absent field
- [ ] Update `--help` long_about for the top-level CLI to document the envelope structure
- [ ] Update `-h` short help to mention the envelope
- [ ] Update `Find` command long_about: change "Returns an array of file objects" to
      describe the envelope
- [ ] Update all `--jq` examples in `--help` text to use `.results` path
- [ ] Update SKILL.md: document the envelope, fix all `--jq` examples
- [ ] Update README.md: fix any JSON snippets and `--jq` examples
- [ ] Audit knowledgebase for stale `--jq` examples (dogfood-results/, backlog/)
- [ ] Update all e2e tests that assert on JSON shape
- [ ] Update `text_jq_filter` tests in output.rs
- [ ] Run full dogfood pass after changes to verify nothing breaks
