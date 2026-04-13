---
title: Iteration 102b — `hyalo lint --fix` auto-remediation
type: iteration
date: 2026-04-13
status: superseded
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

> **Superseded by [[iteration-102c-types-command]]** — the `lint --fix` work in
> this plan (auto-fix, filename-template parsing, type inference, date
> normalization, enum-typo correction, `--dry-run`) was implemented as part of
> iter-102c rather than on a separate branch.

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
- [ ] Implement `--fix`: insert defaults for missing properties with schema defaults
- [ ] Implement `--fix`: correct close enum typos (Levenshtein threshold tuning)
- [ ] Implement `--fix`: infer `type` from filename-template match
- [ ] Implement `--fix`: normalize date formats to ISO 8601
- [ ] Add `--dry-run` to preview fixes without writing
- [ ] Ensure fixes preserve frontmatter key ordering and comments

### Filename-template parsing
- [ ] Parse `{slug}`, `{n}`, `{date}` placeholders
- [ ] Match file paths against templates for type inference
- [ ] Shared module — reused by 102c for `types create/set`

### Tests
- [ ] Unit tests for each fix action in isolation
- [ ] Unit tests for filename-template matching
- [ ] E2E tests for `hyalo lint --fix` on each fix category
- [ ] E2E test: `--dry-run` does not modify files
- [ ] E2E test: `--fix` is idempotent (second run is a no-op)
- [ ] E2E test: `--fix` preserves frontmatter formatting

### Docs & Surfaces (keep all four in sync)
- [ ] Update `hyalo lint --help`: document `--fix`, `--dry-run`
- [ ] Update README.md: add `--fix` examples
- [ ] Update knowledgebase user docs
- [ ] Update skills: mention `lint --fix` as remediation path

### Quality Gates
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Dogfood: run `hyalo lint --fix` on this repo's knowledgebase

## Acceptance Criteria

- [ ] `hyalo lint --fix` auto-fixes: defaults, enum typos, date normalization, type inference
- [ ] `--dry-run` previews without writing
- [ ] Missing required properties without defaults are reported, not fabricated
- [ ] Fixes preserve frontmatter key ordering and formatting
- [ ] README, help texts, knowledgebase docs, and skills updated
