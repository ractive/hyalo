---
type: iteration
title: Iteration 162 — deb/rpm packaging + Cloudsmith publishing
date: 2026-07-11
status: completed
branch: iter-162/linux-packages
tags:
  - iteration
  - ci
  - release
  - packaging
---

# Iteration 162 — Linux packages (deb/rpm) + Cloudsmith

Stacked on [[iteration-161-shared-release-workflow|iteration 161]]'s shared
`ractive/release-workflows@v0.2.0` migration. That shared workflow ships a
`linux-packages` job (native build, then `cargo deb` / `cargo generate-rpm`)
and an optional `cloudsmith` publish job, both gated behind caller inputs.
This iteration turns them on for hyalo, following the pattern already
validated in hoppy (`hoppy-cli/Cargo.toml`'s `[package.metadata.deb]` /
`[package.metadata.generate-rpm]`).

hyalo has a `hyalo completion <shell>` subcommand (clap_complete) that the
release pipeline never packaged. This iteration also fixes that: a
`pre-package-command` generates bash/zsh/fish completions, ships them in the
archives (`extra-archive-paths`), and installs them via deb/rpm asset lines
(hoppy's path conventions). No man pages — hyalo has no clap_mangen and man
pages are explicitly out of scope for now.

## Goal

`hyalo` gets `.deb` and `.rpm` packages built on every release and published
both as GitHub release assets and to the hosted Cloudsmith apt/yum repos at
`ractive/ractive-pkgs`, with no regressions to the existing archive-based
release flow.

## Tasks

- [x] Add `[package.metadata.deb]` and `[package.metadata.generate-rpm]` to
      `crates/hyalo-cli/Cargo.toml` (binary `hyalo`; assets: binary +
      LICENSE + README only, no completions/man)
- [x] Package shell completions: `pre-package-command` generates
      bash/zsh/fish via `hyalo completion` (host build fallback for cross
      targets), `extra-archive-paths: completions`, and completion asset
      lines in both package metadata sections; verified locally via
      `cargo deb`/`cargo generate-rpm` (deb lists all three completion
      files at hoppy-convention paths)
- [x] Enable `enable-linux-packages: true`, `linux-package-crate: hyalo-cli`,
      and `cloudsmith-repo: ractive/ractive-pkgs` in
      `.github/workflows/release.yml`
- [x] Validate `Cargo.toml` parses (`cargo metadata`, `cargo check -p
      hyalo-cli`)
- [x] Locally validate packaging config end-to-end: `cargo deb -p hyalo-cli
      --no-build --no-strip` and `cargo generate-rpm -p crates/hyalo-cli`
      against a macOS-built binary (produces a real `.deb`/`.rpm`; not a
      Linux-installable artifact, but proves asset paths and control
      metadata resolve correctly)
- [x] Confirm `actionlint` passes on the updated `release.yml`
- [x] Run `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`
- [x] Dispatch a `workflow_dispatch` dry run on this branch — builds the
      real deb/rpm on Linux in CI (dry-run skips only the Cloudsmith
      publish step)
- [x] Open PR stacked on iteration 161's PR (`iter-161/shared-release-workflow`
      as base)
- [x] After iteration 161 merges and this PR retargets to `main`: verify a
      real release publishes packages to Cloudsmith

## Acceptance criteria

- [x] `crates/hyalo-cli/Cargo.toml` has valid `[package.metadata.deb]` /
      `[package.metadata.generate-rpm]` sections; local `cargo deb` /
      `cargo generate-rpm` dry runs succeed
- [x] `.github/workflows/release.yml` passes `actionlint` with zero findings
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all pass (no non-CI Rust code changes)
- [x] `workflow_dispatch` dry run on the branch builds `.deb` and `.rpm`
      packages successfully in the `linux-packages` job
- [x] PR is open against `iter-161/shared-release-workflow`, not merged
- [x] PR body documents install instructions (Cloudsmith apt/yum one-liners)
      and notes the stacked-PR relationship

## Notes

- Local `cargo deb`/`cargo generate-rpm` runs on macOS produce a `arm64`
  `.deb` and an `aarch64` `.rpm` from the macOS-format binary — these are
  not installable on Linux, but they do exercise the full metadata
  resolution path (asset paths relative to the crate dir, control file
  fields, `usr/share/doc/hyalo/` layout) and catch TOML/config mistakes
  before CI. The real correctness check is the CI dry run on
  `ubuntu-latest`, which builds a genuine Linux binary first.
- Build artifacts from the local dry run (`target/debian/`,
  `target/generate-rpm/`) are not committed — cleaned up after inspection.
