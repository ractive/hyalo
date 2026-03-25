---
title: "Iteration 43 — Data Quality & Write Fidelity"
type: iteration
date: 2026-03-25
status: planned
branch: iter-43/data-quality
tags: [iteration, data-quality, frontmatter, filtering]
---

# Iteration 43 — Data Quality & Write Fidelity

## Goal

Improve data quality tooling and write fidelity: date-aware property comparisons, inconsistency detection for controlled-vocabulary properties, and reduced frontmatter reformatting on mutations.

## Backlog items

- [[backlog/date-aware-comparison]] (low)
- [[backlog/status-inconsistency-detection]] (low)
- [[backlog/frontmatter-reformatting]] (low)

## Tasks

### Date-aware comparison
- [ ] Detect date-like values on both sides of property comparisons
- [ ] Parse common date formats (ISO 8601, MM/DD/YYYY, YYYY-MM-DD)
- [ ] Compare as dates when both sides parse successfully, fall back to string comparison
- [ ] E2e test: date comparisons produce chronologically correct results

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

- [ ] Date comparisons work correctly on non-ISO date formats
- [ ] Inconsistency detection flags rare property values
- [ ] Frontmatter key order preserved on mutation
- [ ] All quality gates pass
