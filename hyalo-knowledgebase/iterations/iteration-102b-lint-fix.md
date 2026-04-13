---
title: Iteration 102b — `hyalo lint --fix` auto-remediation
type: iteration
date: 2026-04-13
status: completed
branch: iter-102b/lint-fix
tags:
  - iteration
  - schema
  - lint
  - auto-fix
depends-on: iterations/iteration-102a-schema-and-lint.md
---

# Iteration 102b — `hyalo lint --fix`

## Goal

Add auto-remediation to `hyalo lint`. Depends on **[[iteration-102a-schema-and-lint]]** — schema model, validation, and read-only `lint` must land first.

## `--fix` Auto-Fixes

| Fixable | How |
|---------|-----|
| Missing property with a default in schema | Insert the default |
| Close enum typo ("planed" → "planned") | Levenshtein via `strsim` (already a dep) |
| Missing `type` when file path matches a `filename-template` | Infer type from path |
| Normalizable date ("2026-4-9" → "2026-04-09") | Normalize format |

**Not fixable by `--fix`:** missing required properties without defaults — report only. The LLM/skill decides what value to use.

```bash
hyalo lint --fix
hyalo lint --fix iterations/iteration-101-bm25.md
hyalo lint --fix --glob "iterations/*.md"
hyalo lint --fix --dry-run     # preview without writing
```

## Tasks

### Implementation
- [x] Implement `--fix`: insert defaults for missing properties with schema defaults
- [x] Implement `--fix`: correct close enum typos (Levenshtein threshold tuning)
- [x] Implement `--fix`: infer `type` from filename-template match
- [x] Implement `--fix`: normalize date formats to ISO 8601
- [x] Add `--dry-run` to preview fixes without writing
- [x] Ensure fixes preserve frontmatter key ordering and comments

### Filename-template parsing
- [x] Parse `{slug}`, `{n}`, `{date}` placeholders
- [x] Match file paths against templates for type inference
- [x] Shared module — reused by 102c for `types create/set`

### Tests
- [x] Unit tests for each fix action in isolation
- [x] Unit tests for filename-template matching
- [x] E2E tests for `hyalo lint --fix` on each fix category
- [x] E2E test: `--dry-run` does not modify files
- [x] E2E test: `--fix` is idempotent (second run is a no-op)
- [x] E2E test: `--fix` preserves frontmatter formatting

### Docs & Surfaces (keep all four in sync)
- [x] Update `hyalo lint --help`: document `--fix`, `--dry-run`
- [x] Update README.md: add `--fix` examples
- [x] Update knowledgebase user docs
- [x] Update skills: mention `lint --fix` as remediation path

### Quality Gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [x] Dogfood: run `hyalo lint --fix` on this repo's knowledgebase

## Acceptance Criteria

- [x] `hyalo lint --fix` auto-fixes: defaults, enum typos, date normalization, type inference
- [x] `--dry-run` previews without writing
- [x] Missing required properties without defaults are reported, not fabricated
- [x] Fixes preserve frontmatter key ordering and formatting
- [x] README, help texts, knowledgebase docs, and skills updated
