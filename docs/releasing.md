# Releasing hyalo

> Part of the [hyalo](../README.md) documentation. Maintainer-facing.

1. Bump the workspace version and every manifest that must match it (see
   [Version-sync prerequisites](#version-sync-prerequisites)), and bump the
   default `version` input of [`npm-registry.yml`](../.github/workflows/npm-registry.yml)
   to the new version.
2. Rotate the changelog with hyalo itself: `hyalo changelog release X.Y.Z --apply`
   (check that the new footer link is a GitHub compare URL, not `TBD`).
3. Cut the release: `gh release create vX.Y.Z --generate-notes`.

## Version-sync prerequisites

xtask gates in CI fail until these all carry the Cargo workspace version
(`[workspace.package] version` in `Cargo.toml`, plus the `hyalo-core` and
`hyalo-mdlint` path-dependency versions there):

| Manifest | Gate |
| --- | --- |
| `package.json` (root pi package) | `check-pi-package-sync` |
| `pi-package/package.json` | `check-pi-package-sync` |
| `crates/hyalo-cli/templates/pi/package.json` (vendored copy; `just sync-pi-package`) | `check-pi-package-sync` |
| `plugins/hyalo/.codex-plugin/plugin.json` (Codex plugin, DEC-340) | `check-pi-package-sync` |
| `npm/hyalo/package.json` and its platform `optionalDependencies` | `generate-npm-packages --check` (npm Packages workflow) |

The Codex plugin follows the workspace version because it ships the
per-release jev helper (DEC-340). On a release the npm job also refuses a
release tag that differs from `v<Cargo version>`.

## What the release pipeline does

Publishing the release triggers [`release.yml`](../.github/workflows/release.yml),
a thin caller for the shared reusable pipeline in
[ractive/release-workflows](https://github.com/ractive/release-workflows), plus
a local `npm` job. From a single tag, it:

- builds seven targets and runs the test suite on four of them (the other three
  are build-only, `run_tests: false`; native tests already gate every PR, and
  QEMU-emulated runs were slow and flaky):

  | Target | Runner | Tests |
  | --- | --- | --- |
  | `x86_64-unknown-linux-gnu` | ubuntu | yes |
  | `x86_64-unknown-linux-musl` | ubuntu (cross) | yes |
  | `aarch64-unknown-linux-gnu` | ubuntu (cross) | build only |
  | `aarch64-unknown-linux-musl` | ubuntu (cross) | build only |
  | `aarch64-apple-darwin` | macOS | yes |
  | `x86_64-pc-windows-msvc` | Windows | yes |
  | `aarch64-pc-windows-msvc` | Windows | build only |

- packages versioned archives, plus `.deb`/`.rpm` packages, and publishes them
  to the hosted apt/yum repos at Cloudsmith;
- publishes the crates to crates.io (with retry) and updates the Homebrew tap,
  Scoop bucket, winget manifest and the `hyalo-bin` AUR package (Cloudsmith and
  AUR are non-blocking and need the `CLOUDSMITH_API_KEY` / `AUR_SSH_PRIVATE_KEY`
  repo secrets);
- emits CycloneDX SBOMs and GitHub build-provenance attestations for the native
  builds;
- runs the `npm` job: stages the eight npm packages (seven platform packages
  plus `@ractive-ch/hyalo`) from the exact release archives, dry-runs every
  publication, plans an immutable publication (an identical version already on
  the registry is skipped) and publishes with provenance, platform packages
  first.

After the release, verify installation from the registry with
`gh workflow run npm-registry.yml -f version=X.Y.Z`.

### Manual runs (`workflow_dispatch`)

`gh workflow run release.yml` without inputs is a full dry run: every target is
built and packaged and every npm publication is dry-run, nothing is published.
Three mutually exclusive npm modes exist for recovery and bootstrapping; each
requires `-f npm_version=X.Y.Z`, and the run fails unless it equals the Cargo
version of the dispatched ref:

- `publish_npm=true` — build everything, then publish only the npm packages;
- `prepare_npm_bootstrap=true` — sign the eight npm tarballs with GitHub OIDC
  and upload them as an artifact for the initial owner publication;
- `prepare_npm_main_bootstrap=true` — sign and upload only the scoped main
  package, without native builds.

If a downstream step needs to be re-run after a release,
[`publish-crates.yml`](../.github/workflows/publish-crates.yml) re-publishes to
crates.io and [`cloudsmith-republish.yml`](../.github/workflows/cloudsmith-republish.yml)
backfills the Cloudsmith repos.

## Pinning policy

Third-party actions are pinned to a full commit SHA with a `# vX.Y.Z` comment;
Dependabot (`.github/dependabot.yml`) proposes the updates. First-party
reusable workflows and actions — `ractive/release-workflows@v0.2.x` and
`ractive/setup-hyalo@v1` — deliberately float on tags: they are owned and
released together with hyalo, a tag move is a reviewed release of that repo,
and the `lint-kb` jobs dogfood exactly what consumers run (DEC-051 for
setup-hyalo). Dependabot is told to ignore them.

## Package repository hosting

[![OSS hosting by Cloudsmith](https://img.shields.io/badge/OSS%20hosting%20by-cloudsmith-blue?logo=cloudsmith&style=flat-square)](https://cloudsmith.com)

Package repository hosting is graciously provided by [Cloudsmith](https://cloudsmith.com).
Cloudsmith is the only fully hosted, cloud-native, universal package management solution, that
enables your organization to create, store and share packages in any format, to any place, with total
confidence.
