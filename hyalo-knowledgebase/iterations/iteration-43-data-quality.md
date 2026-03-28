---
branch: iter-43/data-quality
date: 2026-03-25
status: superseded
tags:
- iteration
- data-quality
- frontmatter
title: Iteration 43 — Data Quality & Write Fidelity
type: iteration
---

# Iteration 43 — Data Quality & Write Fidelity

## Goal

Improve data quality tooling and write fidelity: orphan detection in summary, inconsistency detection for controlled-vocabulary properties, and reduced frontmatter reformatting on mutations.

## Backlog items

- [[backlog/status-inconsistency-detection]] (low)
- [[backlog/frontmatter-reformatting]] (low)

## Tasks

### Orphan detection in summary
- [x] Add `all_targets()` method to `LinkGraph`
- [x] Add `OrphanSummary` type and extend `VaultSummary`
- [x] Compute orphans (fully isolated files — no links in or out) in summary command
- [x] Update text formatter to display orphans
- [x] E2e tests for orphan detection
- [x] Update README documentation

### Status inconsistency detection
- [ ] Add lint/validation mode (e.g. `hyalo lint` or `hyalo properties --warn-rare`)
- [ ] Flag property values appearing in <2% of files for controlled-vocabulary properties
- [ ] Report suggested correction (the majority value)
- [ ] E2e test with an intentional typo

### Frontmatter key ordering
- [ ] Replace `BTreeMap` with `IndexMap` to preserve insertion order on write
- [ ] Verify mutation round-trip preserves key order
- [ ] E2e test: set/remove cycle doesn't reorder keys

### Quality gates
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`

## Acceptance Criteria

- [x] Fully isolated orphan files (no links in or out) reported in summary
- [ ] Inconsistency detection flags rare property values
- [ ] Frontmatter key order preserved on mutation
- [ ] All quality gates pass
