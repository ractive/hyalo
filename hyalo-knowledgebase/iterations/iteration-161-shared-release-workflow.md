---
type: iteration
title: "Iteration 161 — Migrate release pipeline to shared ractive/release-workflows"
date: 2026-07-10
status: in-progress
branch: iter-161/shared-release-workflow
tags:
  - iteration
  - ci
  - release
  - packaging
---

# Iteration 161 — Shared release workflow migration

Replace hyalo's copy-paste `release.yml` / `publish-crates.yml` with thin
callers into the new shared [[research/release-pipeline-unification|reusable
release pipeline]] at `ractive/release-workflows@v0.1.0`. hyalo, hoppy, and
ff-rdp were three independently-drifting copies of the same skeleton; the
shared workflow fixes the pipeline once and every repo inherits build,
package, SBOM, attestation, and publish logic identically.

## Goal

`.github/workflows/release.yml` and `.github/workflows/publish-crates.yml`
become thin `workflow_call` wrappers with no inline build/publish logic.
Behavior should match or improve on the previous pipeline — no regressions
in target coverage, crates.io retry semantics, or hermetic build provenance.

## Tasks

- [x] Replace `release.yml` with a thin caller (`release` +
      `workflow_dispatch` dry-run triggers)
- [x] Replace `publish-crates.yml` with a thin caller, preserving the
      `ref` dispatch input
- [x] Verify the default 7-target matrix in the shared workflow matches
      hyalo's previous matrix exactly
- [x] Confirm `actionlint` passes on both new workflow files
- [x] Leave `ci.yml`, `quality-gates.yml`, and `.github/release.yml`
      untouched
- [x] Run `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`
- [ ] Open PR documenting behavior deltas (artifact naming, LICENSE/README
      in archives, new SBOM/attestation, crates.io retry preserved)
- [ ] After merge: trigger `workflow_dispatch` dry run to validate
      end-to-end against the real repo

## Acceptance criteria

- [ ] `.github/workflows/release.yml` and
      `.github/workflows/publish-crates.yml` contain no inline
      build/package/publish steps — only `uses:` + `with:`
- [ ] `actionlint` passes with zero findings
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q` all pass (no Rust code
      changes expected)
- [ ] PR is open against `main`, not merged
- [ ] PR body documents all behavior deltas versus the previous pipeline

## Behavior deltas versus the previous pipeline

- **Artifact naming**: `hyalo-<target>.*` becomes `hyalo-v<version>-<target>.*`.
  Homebrew/Scoop manifests are regenerated from `SHA256SUMS` on every
  release, so they pick up the new names automatically. The winget
  `installers-regex` in the shared workflow matches the new pattern.
- **Archive contents**: previously the bare binary only; now `LICENSE` and
  `README.md` (both present at the hyalo workspace root) are copied into
  every archive alongside the binary.
- **New**: CycloneDX SBOM generation (`cargo-cyclonedx`) and
  `actions/attest-build-provenance` build provenance attestation on native
  (non-cross) targets.
- **Preserved**: crates.io publish retry/backoff loop and "already
  uploaded/exists" idempotency handling for `hyalo-mdlint` and `hyalo-cli`;
  hermetic `GIT_COMMIT`/`GIT_COMMIT_DATE` provenance env (still sourced from
  hyalo's own `Cross.toml` passthrough + native `$GITHUB_ENV`); per-target
  `Swatinem/rust-cache` keys; `version-check` and `security` (cargo-audit +
  cargo-deny) gating.
