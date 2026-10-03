---
title: "Iteration 305: CI gates and repository hygiene"
type: iteration
date: 2026-10-04
tags: [iteration, ci, release, hygiene]
status: completed
branch: iter-305/ci-and-hygiene
---

# CI gates and repository hygiene

## Problem

The "Repository hygiene and CI" and "Structural observations" sections of
[[reviews/codebase-review-2026-10-03]] list gaps in the CI and release
plumbing: cargo-deny never runs on PRs, `quality-gates` skips pushes to main,
the Codex plugin manifest is pinned at 0.1.0 outside the version gate, `just
check` runs none of the xtask gates, xtask carries dead stubs, and
`docs/releasing.md`, `CHANGELOG.md` and README contradict the published
releases. This iteration closes those without touching product code.

## Tasks

- [x] Run `cargo deny check` locally, fix what it reports, add a SHA-pinned cargo-deny step to `quality-gates`
- [x] Run `quality-gates` on push to main as well as on PRs (`lint-kb` stays PR-only, `lint-kb-full` push-only)
- [x] Put the Codex plugin manifest under the workspace version gate and set it to the workspace version (DEC-340, docs/releasing.md)
- [x] `just gates` runs every xtask gate CI's `quality-gates` runs, in the same order; `just check` calls it after fmt/clippy/test
- [x] Delete the dead xtask stubs `check-dead-primitives` / `check-todo-annotations` and their references
- [x] Decide `check-behavioral-contracts` (wire into CI or delete) and record DEC-341
- [x] Add `.github/dependabot.yml` (github-actions, cargo, npm for `/npm/hyalo`, and `/npm/jev` if bun lockfiles are supported)
- [x] Comment the jev-helper Node 22 pin (engines floor); every other job uses the same Node 24 pin with a full-version comment
- [x] "Pinning policy" paragraph in docs/releasing.md (first-party reusable workflows float on tags, third-party actions SHA-pinned)
- [x] Correct docs/releasing.md: target table, npm job, AUR, npm `workflow_dispatch` modes and `npm_version` guard, version-sync prerequisites, `npm-registry.yml` default bump (set to 0.24.1)
- [x] CHANGELOG: `[0.22.0]` entry, compare links for 0.21.0/0.20.0, consistent 0.23.0 link; README 0.22.0 and v0.21.0 sentences fixed
- [x] `pi-extension-e2e.sh` fails clearly when esbuild is missing; README licence line links AI_NOTICE (already linked; verified)

## Acceptance criteria

- [x] `cargo deny check` passes locally and in CI
- [x] `just gates` (or its commands run directly) passes
- [x] fmt, clippy `-D warnings` and `cargo test --workspace` pass
- [x] Workflows validate (actionlint or YAML parse)
- [x] `hyalo lint --strict` reports no new findings on the changed knowledgebase files
- [x] CI green on the PR

## Notes

- `cargo deny check` reported nothing to fix (advisories, bans, licences and
  sources ok; duplicate-version warnings only, documented in `deny.toml`).
- Dependabot supports bun's text `bun.lock` through the `bun` ecosystem, so
  `/npm/jev` is covered.
- 0.22.0 and 0.23.0 were published to npm only (verified with `gh release
  list`, `npm view` and crates.io): their CHANGELOG footer links point at npm,
  since there is no tag to compare.
- `decision-log.md` carries five pre-existing HYALO008 errors (`#DEC-080`,
  `#DEC-067`, `#DEC-245`, `#DEC-249` x2 point at headings that do not exist);
  out of scope here, none come from this iteration's additions.
