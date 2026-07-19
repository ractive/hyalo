# Releasing hyalo

> Part of the [hyalo](../README.md) documentation. Maintainer-facing.

1. Bump the workspace version in `Cargo.toml`
2. Rotate the changelog with hyalo itself: `hyalo changelog release X.Y.Z --apply` (then replace the `TBD` footer link with the real compare URL)
3. Cut the release: `gh release create vX.Y.Z --generate-notes`

Publishing the release triggers [`release.yml`](../.github/workflows/release.yml), a thin caller for the shared reusable pipeline in [ractive/release-workflows](https://github.com/ractive/release-workflows). From a single tag, it:

- builds and tests seven targets (Linux x86_64/ARM64 in both glibc and musl, macOS Apple Silicon, Windows x86_64/ARM64);
- packages versioned archives, plus `.deb`/`.rpm` packages, and publishes them to the hosted apt/yum repos at Cloudsmith;
- publishes the crates to crates.io (with retry) and updates the Homebrew tap, Scoop bucket, and winget manifest;
- emits CycloneDX SBOMs and GitHub build-provenance attestations for the native builds.

Rehearse the whole thing without publishing anything via `gh workflow run release.yml` — a `workflow_dispatch` run builds and packages every target as a full dry run. If a downstream step needs to be re-run after a release, [`publish-crates.yml`](../.github/workflows/publish-crates.yml) re-publishes to crates.io and [`cloudsmith-republish.yml`](../.github/workflows/cloudsmith-republish.yml) backfills the Cloudsmith repos.

## Package repository hosting

[![OSS hosting by Cloudsmith](https://img.shields.io/badge/OSS%20hosting%20by-cloudsmith-blue?logo=cloudsmith&style=flat-square)](https://cloudsmith.com)

Package repository hosting is graciously provided by [Cloudsmith](https://cloudsmith.com).
Cloudsmith is the only fully hosted, cloud-native, universal package management solution, that
enables your organization to create, store and share packages in any format, to any place, with total
confidence.
