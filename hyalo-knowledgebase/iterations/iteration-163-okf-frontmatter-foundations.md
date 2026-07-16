---
title: "Iteration 163 — OKF frontmatter foundations (tz timestamps + reserved-file exemption)"
type: iteration
date: 2026-07-16
status: planned
branch: iter-163/okf-frontmatter-foundations
tags: [iteration, okf, schema, frontmatter, lint]
related: [research/okf-open-knowledge-format.md]
priority: 1
---

# Iteration 163 — OKF frontmatter foundations

Foundation for OKF support (see [[okf-open-knowledge-format]]). Fixes the two blocking incompatibilities that make hyalo false-positive on real OKF bundles, independent of any `okf` profile/UI. Everything else (init profile, generators, conformance) builds on this.

## Goal

hyalo can validate/lint a real OKF bundle (e.g. `okf/bundles/crypto_bitcoin`) with **zero false positives** on tz-aware timestamps and reserved files.

## Steps / Tasks

### 1. Timezone-aware datetime constraint

- [ ] Add `datetime-tz` `PropertyConstraint` variant (RFC 3339 with offset, e.g. `2026-05-28T22:44:47+00:00` and `...Z`) in `crates/hyalo-core/src/schema.rs`; keep naive `datetime` unchanged
- [ ] Add `is_datetime_tz` inference/validation in `crates/hyalo-core/src/frontmatter/types.rs` (accept offset `±hh:mm` and `Z`; do not silently accept naive as tz or vice-versa)
- [ ] Wire `datetime-tz` through `hyalo types set --property-type K=datetime-tz` and `types show`
- [ ] Ensure the `HYALO004`-style invalid-datetime lint recognizes `datetime-tz` typed properties
- [ ] Decide + document inference precedence (naive `datetime` vs `datetime-tz` when a bare value is seen) in the type-inference doc/comments

### 2. Reserved-file exemption mechanism

- [ ] Add schema-level `exempt` glob list (e.g. `[schema] exempt = ["**/index.md", "**/log.md"]`) parsed in `crates/hyalo-cli/src/config.rs`
- [ ] Honor `exempt` in schema validation (`validate_on_write`) AND in `hyalo lint` frontmatter/required-type checks — exempt files skip required-`type`/frontmatter-presence rules
- [ ] Allow the bundle-root `index.md` to carry a lone `okf_version` key without tripping undeclared-property (`HYALO002`) — scoped so it applies only to the root index, not arbitrary files
- [ ] Confirm glob matching is vault-relative and cross-platform (Windows separators)

### 3. Tests

- [ ] Unit tests: `is_datetime_tz` accepts offsets + `Z`, rejects naive and garbage; naive `datetime` still rejects tz values unless declared `datetime-tz`
- [ ] Unit tests: `exempt` globs skip validation for `index.md`/`log.md` at any depth; non-exempt files still validated
- [ ] e2e: lint a copied OKF sample bundle → 0 errors on timestamps and reserved files
- [ ] `cargo fmt` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace -q` all green

### 4. Docs sync (same PR)

- [ ] `hyalo types --help` / schema docs list `datetime-tz`
- [ ] `hyalo lint --help` / config docs describe `[schema] exempt`
- [ ] README.md: note tz-aware datetime + exempt globs
- [ ] Update [[okf-open-knowledge-format]] gap #1/#2 status
- [ ] Update bundled skill templates if they enumerate property types

## Acceptance Criteria

- [ ] A real OKF concept doc with `timestamp: '...+00:00'` validates clean when typed `datetime-tz`
- [ ] `index.md` and `log.md` in an OKF bundle are not flagged for missing `type`
- [ ] Root `index.md` with only `okf_version: "0.1"` lints clean
- [ ] Naive `datetime` behavior is unchanged (no regressions in existing vaults)
- [ ] All three quality gates pass; docs updated in the same PR
