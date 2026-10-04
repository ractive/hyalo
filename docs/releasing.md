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
reusable workflows and actions are referenced by tag, not SHA:
`ractive/release-workflows` callers pin exact tags (`@v0.2.0`, `@v0.2.1`), and
`ractive/setup-hyalo@v1` deliberately floats on its major tag so the `lint-kb`
jobs dogfood exactly what consumers run (DEC-051). Both repos are owned and
released together with hyalo, so a tag is a reviewed release of that repo.
Dependabot is told to ignore them.

## Documentation sources

Behaviour is described by hand in several places. No gate compares their
prose with each other or with the binary, so a behaviour change has to touch
each surface that describes it in the same PR (DEC-347).

| Surface | Path | What protects it |
| --- | --- | --- |
| Claude Code skill (installed by `init --claude`) | `crates/hyalo-cli/templates/skill-hyalo.md` | `check-bundled-skills` (skills profile), `check-jq-recipes` |
| Knowledgebase rule (installed by `init --claude`) | `crates/hyalo-cli/templates/rule-knowledgebase.md` | `check-jq-recipes` |
| Pi skill | `pi-package/skills/hyalo/SKILL.md` | `check-pi-package-sync` against its vendored twin in `crates/hyalo-cli/templates/pi/`, `check-bundled-skills`, `check-jq-recipes` |
| Codex skill | `plugins/hyalo/skills/hyalo/SKILL.md` | `check-codex-package` against its vendored twin in `crates/hyalo-cli/templates/codex/` |
| Behaviour claims paragraph | the `<!-- hyalo:start -->` … `<!-- hyalo:end -->` block of `.claude/CLAUDE.md` | `check-jq-recipes` |
| User docs | `README.md`, `docs/*.md` | `check-jq-recipes` (DEC-346); `docs/ci.md`'s OKF block is executed |
| Contributor guide | `CLAUDE.md` | `check-jq-recipes` (DEC-346) |
| Knowledgebase docs | `hyalo-knowledgebase/docs/*.md` | `check-jq-recipes` (DEC-346), `hyalo lint --strict` |
| Changelog | `CHANGELOG.md` | none — `[Unreleased]` is written by hand per PR |
| Command help | `crates/hyalo-cli/src/cli/args.rs` | `check-help-drift`, `check-command-reference`, the `agent_discoverability` e2e size ceiling |

The Pi and Codex skills are gated only against their vendored twins, never
against the Claude template, so a claim corrected in one skill text stays
wrong in the other two until someone edits them. Run `just sync-pi-package`
or `just sync-codex-package` after editing a Pi or Codex skill.

The claims paragraph in `.claude/CLAUDE.md` is the canonical summary of
behaviour decisions. The knowledgebase rule and the pitfalls section of the
Claude skill restate parts of it for agents; when a claim changes, edit all
three in the same PR (DEC-347). `check-jq-recipes` executes every
documented hyalo command carrying `--jq` in these documents and refuses a mutating recipe without
`--dry-run` or a filter reading `.hints` (always `[]` under `--jq`, DEC-313).
It does not check plain, non-`--jq` examples, so keep `--dry-run` on mutating
examples in user docs by hand.

## Package repository hosting

[![OSS hosting by Cloudsmith](https://img.shields.io/badge/OSS%20hosting%20by-cloudsmith-blue?logo=cloudsmith&style=flat-square)](https://cloudsmith.com)

Package repository hosting is graciously provided by [Cloudsmith](https://cloudsmith.com).
Cloudsmith is the only fully hosted, cloud-native, universal package management solution, that
enables your organization to create, store and share packages in any format, to any place, with total
confidence.
