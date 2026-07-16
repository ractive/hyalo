---
title: "Iteration 163 — OKF foundations (tz timestamps, absolute links, reserved-file exemption)"
type: iteration
date: 2026-07-16
status: planned
branch: iter-163/okf-frontmatter-foundations
tags: [iteration, okf, schema, frontmatter, lint]
related: [research/okf-open-knowledge-format.md]
priority: 1
---

# Iteration 163 — OKF foundations

Foundation for OKF support (see [[okf-open-knowledge-format]]). Fixes the three incompatibilities that make hyalo false-positive on real OKF bundles — tz-aware timestamps, bundle-absolute links, reserved files — independent of any `okf` profile/UI. Everything else (init profile, generators, conformance) builds on this.

## Goal

hyalo can validate/lint a real OKF bundle (e.g. `okf/bundles/crypto_bitcoin`) with **zero false positives** on tz-aware timestamps, bundle-absolute links, and reserved files.

## Steps / Tasks

### 1. Timezone-aware datetime constraint

- [ ] Add `datetime-tz` `PropertyConstraint` variant (RFC 3339 with offset, e.g. `2026-05-28T22:44:47+00:00` and `...Z`) in `crates/hyalo-core/src/schema.rs`; keep naive `datetime` unchanged
- [ ] Add `is_datetime_tz` inference/validation in `crates/hyalo-core/src/frontmatter/types.rs` (accept offset `±hh:mm` and `Z`; do not silently accept naive as tz or vice-versa)
- [ ] Wire `datetime-tz` through `hyalo types set --property-type K=datetime-tz` and `types show`
- [ ] Ensure the `HYALO004`-style invalid-datetime lint recognizes `datetime-tz` typed properties
- [ ] Decide + document inference precedence (naive `datetime` vs `datetime-tz` when a bare value is seen) in the type-inference doc/comments
- [ ] Handle both YAML spellings found in official OKF material: quoted `'2026-05-28T22:44:47+00:00'` (sample bundles) AND unquoted `2026-05-28T14:30:00Z` (blog example) — verify the YAML parser doesn't special-case unquoted timestamps before hyalo sees them

### 2. Bundle-absolute link resolution (spec §5 recommended form)

SPEC §5: links starting with `/` are bundle-root-relative and the **recommended** form ("stable when documents are moved"). `strip_site_prefix` (`crates/hyalo-core/src/link_graph.rs:569`) already falls back to vault-root resolution when the prefix doesn't match, so these mostly work — but the auto-derived `site_prefix` (vault dirname) mis-strips when a bundle dir shares its name with a top-level subdir (e.g. bundle root `tables/` breaks `/tables/x.md` → `x.md`).

- [ ] Define + document the `site_prefix` setting an OKF vault should use for pure bundle-root resolution (clarify current `--site-prefix ""` "disable" semantics vs pass-through; add an explicit form if needed)
- [ ] Test: `/tables/customers.md` resolves from bundle root in `links`, `backlinks`, `find --broken-links`
- [ ] Test: bundle dir named like a top-level subdir does not mis-strip absolute links

### 3. Reserved-file exemption mechanism

- [ ] Add schema-level `exempt` glob list (e.g. `[schema] exempt = ["**/index.md", "**/log.md"]`) parsed in `crates/hyalo-cli/src/config.rs`
- [ ] Honor `exempt` in schema validation (`validate_on_write`) AND in `hyalo lint` frontmatter/required-type checks — exempt files skip required-`type`/frontmatter-presence rules
- [ ] Allow the bundle-root `index.md` to carry a lone `okf_version` key without tripping undeclared-property (`HYALO002`) — scoped so it applies only to the root index, not arbitrary files
- [ ] Confirm glob matching is vault-relative and cross-platform (Windows separators)
- [ ] Note: `exempt` is logically "bind to no schema" — iter-167's `[schema.bind]` may later subsume it as `= "none"` sugar ([[path-bound-schemas]]); keep `exempt` simple here, don't pre-build the general mechanism

### 4. Tests

- [ ] Unit tests: `is_datetime_tz` accepts offsets + `Z` (quoted and unquoted YAML forms), rejects naive and garbage; naive `datetime` still rejects tz values unless declared `datetime-tz`
- [ ] Unit tests: `exempt` globs skip validation for `index.md`/`log.md` at any depth; non-exempt files still validated
- [ ] e2e: lint a copied OKF sample bundle → 0 errors on timestamps, reserved files, and absolute links
- [ ] `cargo fmt` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace -q` all green

### 5. Docs sync (same PR)

- [ ] `hyalo types --help` / schema docs list `datetime-tz`
- [ ] `hyalo lint --help` / config docs describe `[schema] exempt`
- [ ] README.md: note tz-aware datetime + exempt globs
- [ ] Update [[okf-open-knowledge-format]] gap #1/#2 status
- [ ] Update bundled skill templates if they enumerate property types

### 6. Retrospective (learnings-propagation — do this LAST, always)

- [ ] Review the remaining profile iterations ([[iteration-164-okf-init-profile-and-skill]] through [[iteration-169-changelog-profile]]) against implementation learnings — update their scope/design/tasks before starting the next iteration

## Acceptance Criteria

- [ ] A real OKF concept doc with `timestamp: '...+00:00'` validates clean when typed `datetime-tz`
- [ ] `index.md` and `log.md` in an OKF bundle are not flagged for missing `type`
- [ ] Root `index.md` with only `okf_version: "0.1"` lints clean
- [ ] Bundle-absolute links (`/x/y.md`) resolve correctly in an OKF-configured vault, including the dirname-collision edge case
- [ ] Naive `datetime` behavior is unchanged (no regressions in existing vaults)
- [ ] All three quality gates pass; docs updated in the same PR
