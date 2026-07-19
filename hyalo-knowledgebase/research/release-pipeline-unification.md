---
title: Release Pipeline Unification — hyalo / hoppy / ff-rdp Gap Analysis
type: research
date: 2026-07-10
tags:
  - release
  - ci
  - packaging
  - linux
status: active
---

# Release Pipeline Unification — hyalo / hoppy / ff-rdp

Gap analysis of the three release pipelines and options for adding more
Linux packages and unifying publishing. Sources: workflow analysis of all
three repos + web research on external tooling (July 2026).

## Current state

All three pipelines are copy-paste descendants of the same skeleton:
release-published trigger → `version-check` → `security` (cargo-audit +
cargo-deny) → build matrix → `release` (SHA256SUMS + GitHub assets) →
crates.io (retry/backoff for index lag) + Homebrew tap + Scoop bucket,
using the same pinned actions (checkout v4.3.1, dtolnay/rust-toolchain,
rust-cache v2.9.1, taiki-e/install-action). Version bump is manual in
`[workspace.package]`, release cut via tag + GitHub Release.

### Feature matrix

| Feature | hyalo | hoppy | ff-rdp |
| --- | --- | --- | --- |
| Build targets | 7 | 5 | 6 |
| musl static builds | x86_64 + aarch64 | none (OpenSSL dep) | x86_64 + aarch64 |
| aarch64-linux-gnu | yes | yes | no |
| deb / rpm | no | yes (x86_64 only) | no |
| Homebrew tap | yes | yes | yes |
| Scoop bucket | yes | yes | yes |
| winget | yes | no | yes (non-blocking) |
| crates.io | 3 crates | 3 crates | 2 crates |
| SBOM (CycloneDX) | no | no | yes (native targets) |
| Sigstore attestation | no | no | yes (native targets) |
| Completions + man pages in archives | no | yes | no |
| Hermetic GIT_COMMIT provenance | yes | yes | no |

Each repo has something the others lack: hoppy has deb/rpm + completions
+ man pages; ff-rdp has SBOM + attestations; hyalo has the widest target
matrix + hermetic provenance env vars.

## Q1: How hard are more Linux packages / distros?

Key insight: "more distros" does not mean more formats. **deb** covers
Debian/Ubuntu/Mint, **rpm** covers Fedora/RHEL/openSUSE, and a **musl
static binary** covers everything else. Effort ladder:

1. **deb + rpm as release assets — cheap (~1 day per repo).** hoppy
   already has the full pattern: `[package.metadata.deb]` +
   `[package.metadata.generate-rpm]` in the CLI crate's Cargo.toml plus
   one `linux-packages` CI job running `cargo deb` / `cargo generate-rpm`
   (both tools actively maintained, releases in May 2026). Port to hyalo
   and ff-rdp is mostly copy-paste. aarch64 variants possible from the
   existing cross-built binaries via `--target`.
2. **AUR — cheap.** `KSXGitHub/github-actions-deploy-aur` pushes a
   PKGBUILD with an SSH key; no human review.
3. **Hosted apt/yum repos (real `apt install` + updates) — medium.**
   openSUSE Build Service builds + hosts signed repos for 20+ distro
   families free, but has the steepest learning curve (.spec/.dsc, `osc`).
   Cloudsmith has a free OSS hosting policy (~50 GB). Only worth it on
   user demand; GitHub-release .deb/.rpm assets cover most needs.
4. **snap — medium.** Classic confinement needs a one-time human forum
   review; automatable afterwards.
5. **flatpak — skip.** Wrong tool for CLIs (`flatpak run org.x.Tool`
   invocation, upstream declined to improve it).
6. **Alpine apk — skip.** musl static binary already covers Alpine.
7. **Nix — passive.** Wait for community packaging or write one
   `buildRustPackage` derivation.

Blocker: hoppy links OpenSSL (`Cross.toml` installs `libssl-dev`), which
is why it has no musl targets. Unifying the target matrix requires either
`openssl/vendored` or migrating hoppy to rustls.

## Q2: Unification options

### Option A — Harmonize by porting (minimal)

Port hoppy's deb/rpm job to hyalo + ff-rdp, ff-rdp's SBOM/attestation to
the others, winget to hoppy. Cheap, no new tools, but leaves three
divergent copies that keep drifting.

### Option B — Org-level reusable workflow (recommended)

Extract one `workflow_call` release workflow into a shared repo (e.g.
`ractive/release-workflows`), parameterized by: binary name, ordered
crates-to-publish list, target matrix, winget identifier, feature flags
(deb/rpm, SBOM, completions). Callers pass `secrets: inherit`, so each
repo keeps its own 4 PATs (CARGO_TOKEN, HOMEBREW_TAP_TOKEN,
SCOOP_BUCKET_TOKEN, WINGET_TOKEN). The near-identical hand-rolled
Homebrew/Scoop generation scripts become composite actions.

- Keeps all battle-tested logic (crates.io retry, hermetic provenance,
  per-target cache keys, Windows stack size).
- Fix once, all three repos benefit; drift becomes impossible.
- Bonus: attestations from a shared reusable workflow bind provenance to
  one identity — GitHub's documented path to SLSA v1 Build L3.
- Constraint: `id-token: write` + `attestations: write` needed in both
  caller and callee; secrets flow only one nesting hop.

### Option C — GoReleaser + release-plz (biggest rewrite)

GoReleaser OSS (v2.17, July 2026; Rust support since v2.5) natively does
deb/rpm/apk/ArchLinux (embedded nFPM), Homebrew, winget, Scoop, AUR,
snap from one `.goreleaser.yml`; cross-compiles via cargo-zigbuild.
release-plz (active, June 2026) covers version-bump PRs, changelog, and
ordered crates.io publishing. Risks: Rust builder still maturing, **cargo
workspace support is its weak point** (needs `-p` flags; all three repos
are multi-crate workspaces publishing 2–3 crates), MSI/nightly/monorepo
features are Pro-paid, and the existing custom logic gets thrown away.

### Option D — cargo-dist: not a fit

Maintained again after the 2025 Astral-fork wobble (0.32, May 2026), but
has **no winget, no deb/rpm, no crates.io publish** — all would remain
custom bolt-ons, so it adds framework risk without removing any of the
existing custom code.

## Recommendation

Phased B-over-A:

- [ ] Phase 1 (~1 day/repo): port hoppy's deb/rpm pattern to hyalo and
      ff-rdp as release assets; optionally add completions/man pages
      (needs clap_mangen xtask like hoppy's).
- [ ] Phase 2 (~2–3 days): extract `ractive/release-workflows` with one
      reusable release workflow + composite actions for
      homebrew/scoop/winget/crates-io; migrate all three repos.
      Standardize SBOM + attestations everywhere.
- [ ] Phase 3 (on demand): release-plz for automated version/changelog
      PRs; AUR (cheap); hosted apt/rpm repo via Cloudsmith OSS or OBS
      only if users ask; resolve hoppy's OpenSSL→rustls for musl parity.

GoReleaser remains the fallback if the shared workflow grows unwieldy,
re-evaluate once its workspace support matures.

## Outcome (2026-07-10)

Phase 2 was implemented first, same day (see [[decision-log]] DEC-048):

- <https://github.com/ractive/release-workflows> created — reusable
  `release.yml` + `publish-crates.yml`, actionlint + zizmor CI, and an
  end-to-end selftest (dry-run against a bundled fixture crate, 4 targets,
  incl. deb/rpm + SBOM + multi-line pre-package-command + BIN_PATH).
- Released v0.1.0 → v0.1.3 within hours; each bump fixed a real bug found
  by dry-run verification: eager reusable-workflow permission validation,
  SBOM coverage regression (ff-rdp ships two crate SBOMs → `sbom-packages`
  input), multi-line `pre-package-command` flattening (→ `eval`),
  `cargo run -p` ambiguity for multi-binary packages (→ `--bin`), and
  linux-packages binary path (→ exported `BIN_PATH`). hoppy's man-page
  generation additionally had to stay off Windows (debug xtask overflows
  the 1 MB MSVC stack — parity with its old pipeline).
- Final verification: dry-run releases green on v0.1.3 for all three
  repos (hyalo run 29125077604, ff-rdp 29125085623, hoppy 29126525364).
- Migration PRs (each with a `workflow_dispatch` dry-run trigger):
  hyalo [#188](https://github.com/ractive/hyalo/pull/188),
  hoppy [#91](https://github.com/ractive/hoppy/pull/91),
  ff-rdp [#157](https://github.com/ractive/ff-rdp/pull/157).
- Pre-existing bugs fixed by the PRs: hoppy and ff-rdp both lacked
  `Cross.toml` passthrough for `GIT_COMMIT`/`GIT_COMMIT_DATE`, so their
  cross-compiled binaries embedded container-local provenance; hoppy's
  Windows CLI tests overflow the default MSVC stack (its release matrix
  never ran tests — parity preserved, fix tracked separately).
- Phase 1 (deb/rpm for hyalo + ff-rdp) is now a per-repo flag flip:
  `enable-linux-packages: true` + `[package.metadata.deb]` /
  `[package.metadata.generate-rpm]` sections.

## Live (2026-07-11)

Everything above shipped the same night: hyalo v0.17.0, hoppy v0.5.0, and
ff-rdp v0.3.0 all released green on the shared pipeline. All three now
publish deb/rpm to Cloudsmith (`ractive/hyalo`) and ship packaged
shell completions; hoppy additionally gained musl static builds (the
"links OpenSSL" premise was vestigial — see hoppy iter-80) and a winget
bootstrap submission (microsoft/winget-pkgs#400670). Remaining manual
steps live in hoppy's `iteration-80-musl-targets-winget` plan: Cloudsmith
repo visibility → open-source, WINGET_TOKEN + caller identifier after
moderation, AUR account.

## Key sources

- GoReleaser Rust builder: <https://goreleaser.com/customization/builds/rust/>
- cargo-dist config (no winget/deb/rpm): <https://axodotdev.github.io/cargo-dist/book/reference/config.html>
- release-plz: <https://release-plz.dev/>
- cargo-deb: <https://github.com/kornelski/cargo-deb>
- cargo-generate-rpm: <https://github.com/cat-in-136/cargo-generate-rpm>
- nFPM: <https://nfpm.goreleaser.com/>
- Reusable workflows: <https://docs.github.com/en/actions/how-tos/reuse-automations/reuse-workflows>
- SLSA L3 via reusable workflows: <https://docs.github.com/actions/security-guides/using-artifact-attestations-and-reusable-workflows-to-achieve-slsa-v1-build-level-3>
- OBS cross-distro: <https://en.opensuse.org/openSUSE:Build_Service_cross_distribution_howto>
- Cloudsmith OSS policy: <https://help.cloudsmith.io/docs/open-source-hosting-policy>
- Reference setup (GoReleaser + release-plz): <https://blog.orhun.dev/automated-rust-releases/>
