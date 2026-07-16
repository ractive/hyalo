---
title: "Iteration 166 — OKF conformance lint profile"
type: iteration
date: 2026-07-16
status: planned
branch: iter-166/okf-conformance-lint
tags: [iteration, okf, lint, conformance, validation]
related: [research/okf-open-knowledge-format.md]
priority: 4
depends-on: iteration-165-okf-index-and-log-generators
---

# Iteration 166 — OKF conformance lint profile

Positions hyalo as *the* OKF validator the ecosystem currently lacks. Encodes SPEC §9 as a lint profile, respecting OKF's permissive-consumption model (warn, don't reject). See [[okf-open-knowledge-format]].

## Goal

`hyalo lint --profile okf` reports exactly the SPEC §9 conformance status of a bundle — erroring only on true violations, warning on everything the spec says MUST NOT be rejected.

## Steps / Tasks

### 1. Conformance profile

- [ ] `--profile okf` (or an `okf`-tagged rule bundle) enabling the §9 checks in `crates/hyalo-mdlint`
- [ ] Rule: every non-reserved `.md` has a parseable YAML frontmatter block (error if absent/unparseable)
- [ ] Rule: every such block has a non-empty `type` (error) — reuse iter-163 `exempt` for reserved files
- [ ] Rule: reserved files follow §6/§7 structure when present (`index.md` link-list shape; `log.md` date grouping) — warn
- [ ] Ensure broken cross-links are **warn**, unknown `type`/extra keys are **allowed** (no error), per permissive model

### 2. Optional augmentation guards (parity with reference_agent)

- [ ] Warn when an edit would drop an existing `#` heading or shrink a `# Schema` field set / `# Citations` count (best-effort, diff-aware via `--files-from`)
- [ ] Keep these advisory (warn), off by default outside the okf profile

### 3. Tests

- [ ] e2e: all three committed sample bundles report conformant (0 errors) under `--profile okf`
- [ ] e2e: a doc missing `type` → error; a broken link → warn (not error); an unknown `type` → clean
- [ ] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 4. Docs sync (same PR)

- [ ] `hyalo lint --help` documents `--profile okf`; `hyalo lint-rules list` shows the okf rules
- [ ] README.md: "Validate an OKF bundle" section (`hyalo lint --profile okf`), note the warn-not-reject stance
- [ ] Update the `okf` skill to include validation in the loop
- [ ] Update [[okf-open-knowledge-format]] gap #5 status → done; mark research follow-through complete

## Acceptance Criteria

- [ ] `hyalo lint --profile okf` matches SPEC §9: errors only on missing frontmatter / missing `type`; warns on reserved-file structure and broken links; never errors on unknown types/keys
- [ ] All three official sample bundles pass clean
- [ ] Quality gates pass; docs + skill updated in the same PR
