---
title: Iteration 107 — Lint/Types Surface Sync (hints, docs, skill, README)
type: iteration
date: 2026-04-14
tags:
  - iteration
  - lint
  - types
  - schema
  - docs
  - hints
  - dogfood
status: in-progress
branch: iter-107/surface-sync
---

# Iteration 107 — Lint/Types Surface Sync

## Goal

Close all documentation and UX gaps left by iterations 102a–c. After this iteration, `hyalo lint`, `hyalo lint --fix`, and `hyalo types` are fully discoverable through hints, help texts, CLAUDE.md, the hyalo skill, README, and knowledgebase docs.

## Background

Post-merge review of 102a/b/c found:
- `lint` and `types` commands produce **no drill-down hints**
- `summary` / `properties` don't cross-reference lint/types in their hints
- CLAUDE.md, `.claude/CLAUDE.md`, and the hyalo skill don't mention lint/types
- README.md is missing `lint --fix` / `--dry-run` documentation and the "What it does" section omits schema features
- Knowledgebase `docs/schema-and-lint.md` has no `--fix` coverage
- Broken wikilinks in iteration 102a/b/c files
- Minor: duplicate `is_iso8601_date()` in `filename_template.rs` and `lint.rs`

## Tasks

### Hints
- [x] Add `Lint` and `Types` variants to `HintSource` enum in `hints.rs`
- [x] Implement `hints_for_lint()` — suggest `lint --fix`, `types list`, `find --property type=...`
- [x] Implement `hints_for_types()` — per-subcommand hints (e.g., `types show` → `lint`, `types list` → `types show <name>`)
- [x] Wire hint context in `run.rs` dispatch for `Commands::Lint` and `Commands::Types`
- [x] Add `lint` and `types` cross-references to `summary` and `properties` hint outputs
- [x] Add e2e hint tests for lint and types in `e2e_hints.rs`

### Documentation surfaces (keep all four in sync)
- [x] Update `CLAUDE.md` — add `hyalo lint`, `hyalo lint --fix`, `hyalo types` with examples
- [x] Update `.claude/CLAUDE.md` — add lint/types to the brief examples
- [x] Sync `templates/skill-hyalo.md` → active skill at `~/.claude/skills/hyalo/SKILL.md`
- [x] Update README.md "What it does" section — mention schema validation, lint, types
- [x] Add `lint --fix` and `--dry-run` documentation to README.md lint section
- [x] Update knowledgebase `docs/schema-and-lint.md` — add `--fix` section with examples

### Cleanup
- [x] Fix broken wikilinks in iteration 102a/b/c files (`hyalo links fix`)
- [x] Extract shared `is_iso8601_date()` to `hyalo_core` (deduplicate from `filename_template.rs` and `lint.rs`)

### Quality Gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`

## Acceptance Criteria
- [x] `hyalo lint` output includes at least 2 drill-down hints
- [x] `hyalo types list` output includes at least 2 drill-down hints
- [x] `hyalo summary` hints mention `lint` when a schema is defined
- [x] CLAUDE.md contains lint and types examples
- [x] README.md documents `lint --fix` with at least one example
- [x] `docs/schema-and-lint.md` has a `--fix` section
- [x] `hyalo links check` reports zero broken links in iteration files
- [x] All quality gates pass

## Retrospective Note

Iterations 102a/b/c were split into 3 PRs but could have been 2 (schema+lint+fix, then types). The parallel 102b/102c run caused 102c to absorb 102b's scope. Key lesson: tightly coupled features that share a schema foundation are better landed together. Documentation sync ("Docs & Surfaces" tasks) was present in iteration plans but treated as optional by agents — this iteration exists to enforce it.
