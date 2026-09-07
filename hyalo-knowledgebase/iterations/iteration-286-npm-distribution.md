---
type: iteration
title: "Iteration 286 — npm distribution: launcher and per-platform binary packages"
date: 2026-09-06
status: in-progress
tags: [iteration, npm, distribution, release]
branch: iter-286/npm-distribution
priority: 2
related:
  - "[[research/npm-package-and-typed-typescript-api]]"
  - "[[iterations/iteration-237-pi-package-distribution]]"
  - "[[iterations/iteration-287-typed-typescript-api]]"
  - "[[decision-log]]"
---

# Iteration 286 — npm distribution: launcher and per-platform binary packages

## Goal

`npm install @ractive-ch/hyalo` on any supported machine, with no Homebrew step and no download script: the
esbuild / Biome pattern from [[research/npm-package-and-typed-typescript-api]]. One main
package `@ractive-ch/hyalo` whose `hyalo` bin is a small JS launcher, plus one package per release target under
the `@ractive-ch` scope (org created 2026-09-06, owner `ractive.ch`, two-factor auth on),
selected by npm through `os` / `cpu` / `libc` in `optionalDependencies`. Needs **no
TypeScript types** — it is independent of 285 and 287.

Settled inputs (all in the research note): registry is npmjs.com with trusted publishing
(OIDC), no long-lived token; both glibc and musl Linux packages ship; Intel macOS is not built
and the launcher says so; version = Cargo workspace version, platforms published before the
main package, exact pins; `hoppy` and `ff-rdp` reuse the same scope and layout later.

## Tasks

- [x] TASK-1: DEC weakening the "all code stays in Rust — no polyglot tooling" rule: it was
      written against Bun tests verifying Rust code; `npm/` and `pi-package/` may hold
      TypeScript because they *are* the JavaScript deliverable. Update `CLAUDE.md` in the same
      PR.
- [x] TASK-2: `npm/hyalo/` — `package.json` (`bin`, `optionalDependencies` for the seven
      targets, `engines`, `files`), `bin/hyalo.js` launcher that resolves the platform package,
      execs the binary with inherited stdio and exit code, and fails with a clear message naming
      `os`/`cpu`/`libc` and pointing at `cargo install hyalo-cli` when no package matches (Intel
      macOS, unsupported libc). README with install and the platform table.
- [x] TASK-3: `npm/platforms/` — a generator (an `xtask` subcommand, Rust) that writes one
      platform package per target from a template: name `@ractive-ch/hyalo-<os>-<cpu>[-musl]`,
      `os`/`cpu`/`libc` fields, the binary, the licence. Version taken from `Cargo.toml`.
- [ ] TASK-4: `release.yml` — after the archives exist, unpack each into its platform package,
      `npm publish --provenance` the seven platform packages, then the main package. Configure
      trusted publishing for each of the eight packages on npmjs.com (one-time, manual, James).
      A dry-run path (`npm pack` + `npm publish --dry-run`) runs on every PR touching `npm/` or
      the workflow.
- [x] TASK-5: version gate: extend the `check-pi-package-sync` xtask (or the gate 285 adds) so
      `npm/hyalo/package.json`, every platform package and `pi-package/package.json` equal the
      Cargo workspace version.
- [ ] TASK-6: verify on real machines: `npm install @ractive-ch/hyalo` on macOS arm64, Linux x64 glibc,
      Linux x64 musl (Alpine container) and Windows x64; `npx hyalo --version` prints the
      release version. Record timings for a musl vs glibc `summary` on MDN if a musl host is at
      hand (the allocator question in the note).
- [x] TASK-7: docs: README install section, `skill-hyalo.md`, the knowledgebase research note
      outcome. Gates: `cargo fmt`, clippy, `cargo test --workspace -q`, every xtask `check-*`,
      `hyalo lint --strict`.

## Acceptance criteria

- [ ] `npm install @ractive-ch/hyalo && npx hyalo --version` works on the four verified platforms with
      exactly one platform package installed.
- [x] On an unsupported platform the launcher exits non-zero with the message naming the
      platform and the cargo fallback.
- [ ] A release publishes eight packages at the Cargo version, platforms first, with provenance.
- [x] The version gate fails CI when any `package.json` disagrees with Cargo.
- [x] The polyglot DEC is filed and `CLAUDE.md` reflects it.
- [x] Gates green.

## Plan reconciliation — 2026-09-07, iteration 285

Iteration 285 added Cargo-version enforcement to `check-pi-package-sync` for the root,
canonical pi and embedded pi manifests, now all 0.22.0. TASK-5 should extend that existing
gate to the eight npm packages and their exact platform dependency pins. The launcher
does not depend on the new typed output contracts. Publishing, trusted-publisher setup
and real-platform registry installation remain unfinished external requirements.

## Partial implementation — 2026-09-07

DEC-332, the launcher, seven platform manifests, Rust metadata/staging generator,
version checks, PR packaging checks, release workflow and installation documentation
are implemented on `iter-286/npm-distribution` as repository preparation. The
iteration remains `in-progress` until the external acceptance criteria are verified.

Local validation passed: 4,894 Rust tests, zero failures and two existing ignored
doctests; strict workspace Clippy; Windows-target xtask compilation; nine launcher
tests; actionlint; all implemented xtask gates; package-content checks and publication
dry runs for all eight packages. A temporary local macOS arm64 tarball installation
ran the real CLI. Foreign-target fixture bytes verify packaging only. The two existing
stub gates remain unimplemented; the jq gate cannot exercise its MADR recipe without
`docs/decisions`; full-vault lint retains four existing warnings.

Independent review fixes cover launcher cancellation, integrity-verified publication
retries, Windows command selection and compilation, and CRLF metadata comparison.
The generator now accepts LF/CRLF differences while preserving all other bytes.
Regression coverage verifies CRLF metadata checking and staging, rejects real content
drift and missing files, and preserves lone-CR and invalid-byte distinctions. Fresh
full review found no actionable defects in the repository-preparation changes.

TASK-4 is partial: workflow and local dry runs exist, but owner bootstrap, trusted
publisher configuration and actual publication have not happened. TASK-6 is unperformed:
public-registry installs on macOS arm64, Linux x64 glibc, Alpine musl and Windows x64
are still required. Local fixtures and compilation checks do not satisfy these tasks.
The owner login was verified as `ractive.ch`. On 2026-09-07 the user authorized
Hyalo's eight-package npm bootstrap, publication and GitHub trusted-publisher setup.
No token secret is required for the planned GitHub OIDC publisher.

Resume external completion only with the required authority and real native artifacts.
Bootstrap the eight package names, configure GitHub OIDC for `ractive/hyalo` and
`release.yml` with direct publishing enabled, publish an authorized release, and
record the four-platform registry checks and provenance. Agree on bootstrap/release
versions before publishing: npm versions are immutable. Retain this noncompleted
status until external acceptance is fulfilled. Iteration 287 remains blocked by the
full iteration-286 prerequisite.

## Hyalo-only publishing preparation — 2026-09-07

PR #336 merged the repository preparation after all eleven runnable GitHub checks
passed. The ordinary release workflow also publishes to other distribution channels
and repositories. To preserve the Hyalo-only scope, manual dispatch now has an
explicit `publish_npm` option, defaulting to false, with `npm_version` required to
match Cargo before publication. Every manual dispatch keeps the reusable native
release workflow in dry-run mode; only npm publication can opt in. Default manual
runs remain unpublished, and normal published-release behavior is unchanged.

This prepares the restricted publishing path; it does not complete owner bootstrap,
trusted publishing, actual publication or registry platform verification. The user
explicitly deferred `homefinder-eco-mcp`; iteration 287's consumer task remains open.

## Authorized bootstrap preparation — 2026-09-07

The npm-only bootstrap path builds the real 0.22.0 native archives, packs all eight
packages and signs their exact tarball digests in GitHub Actions. The npm owner can
then upload those unchanged tarballs with `--provenance-file` before configuring
trusted publishers. This initial upload uses owner authentication and CI provenance;
it does not demonstrate OIDC-authenticated publication. No placeholder version is
needed. Keep publication and platform acceptance unchecked until verified.

A separate manual registry workflow will install the public package on macOS arm64,
Linux x64 glibc, Windows x64 and Alpine x64 musl, checking the actual installed native
dependency and CLI version. It does not publish packages. These preparations preserve
iteration 287's full prerequisite and iteration 288's deferred desktop verification.

## Native-build recovery — 2026-09-07

PR #338 merged bootstrap signing and registry verification preparation after all
eleven runnable PR checks passed. Native build `34090552288` then failed its strict
Windows x64 concurrency observer: `File::open` returned access denied (OS error 5)
while writers replaced the same note. Six other native targets completed. Signing
and npm publication did not run.

The recovery adds a private Windows-only retry policy to the three frontmatter
file-open paths: at most five attempts, 10 ms between attempts, only raw errors 5
and 32. Persistent and unrelated errors retain normal failure behavior; parsing,
body reads and atomic writes are not retried. The strict concurrency observer stays
unchanged. Deterministic injected-open tests cover the retry boundary; native Windows
CI and a new same-commit signed build remain required before publication proceeds.

This recovery preserves iteration 287's full prerequisite and iteration 288's
deferred desktop verification. No external acceptance criterion is waived.

## Verified artifacts and authentication blocker — 2026-09-07

PR #339 merged the Windows open retry after fresh review, 4,899 local tests and
all seven applicable PR checks passed, including native Windows tests. The unchanged
npm workflow paths correctly did not trigger its four additional checks.

Build `34092918271` at merge `6f6ff85529b64688818a43143e102fcc05f7298e` then passed
all seven native targets and signed all eight 0.22.0 tarballs. Their SHA-512 digests,
Sigstore signatures, GitHub workflow identity, exact source commit and run were
verified. Binary formats match the seven target architectures; the macOS arm64
binary also ran from a fresh directory and reported the expected version and commit.
These are build and artifact checks, not public-registry installation acceptance.

The initial owner upload reached npm's browser authentication prompt, then ended
with an HTTP 404 from its authentication completion endpoint. Subsequent public
version queries returned 404 for all eight packages. No publication succeeded and
no trusted publisher was configured. Resume with attended npm browser authentication
using the preserved, verified tarballs; no rebuild or new token is required.
TASK-4, TASK-6 and the publication/registry acceptance criteria remain unfinished.

## Partial publication and naming blocker — 2026-09-07

Attended owner authentication succeeded on retry. All seven platform packages at
0.22.0 are now public under `@ractive-ch`, uploaded from the verified build artifacts
in platform-first order. Their public registry integrity matches the verified
tarballs and each version exposes provenance attestations. These uploads used owner
authentication with CI-signed provenance, not OIDC-authenticated publication.

The final `hyalo@0.22.0` upload failed with npm E403: the registry considers `hyalo`
too similar to the existing package `yalc`. A subsequent public version query still
returned 404 for `hyalo`. Authentication alone cannot resolve this name rejection.
No trusted publishers were configured and the four-platform registry installation
workflow has not run. TASK-4, TASK-6 and both external acceptance criteria stay open.

Resume requires either npm allowing the unscoped name or a user decision to change
the main package name, for example to the existing organization's `@ractive-ch/hyalo`.
A 404 for that scoped version is not proof npm will accept publication. A rename
requires reviewed manifest, generator, workflow, documentation and dependent-plan
updates plus newly signed main-package bytes. Preserve the seven immutable platform
versions and their existing artifacts; do not republish rebuilt bytes over them.
Iteration 287 remains blocked by full iteration 286; Homefinder and iteration 288's
desktop verification remain deferred. No acceptance requirement is waived.

## Scoped main recovery authorization — 2026-09-07

After npm rejected the unscoped main name as too similar to `yalc`, the user
authorized `@ractive-ch/hyalo` as the canonical main package. The CLI executable
remains `hyalo`, and the seven public platform packages remain immutable at 0.22.0
with their previously verified bytes. Repository recovery must generate and sign
only the newly scoped main tarball; it must not rebuild or republish those platform
versions.

The recovery workflow adds a mutually exclusive `prepare_npm_main_bootstrap` mode.
It skips the reusable native build, verifies generated metadata, packs the tracked
`npm/hyalo` source as `ractive-ch-hyalo-0.22.0.tgz`, dry-runs publication, signs that
one tarball and uploads it with the existing `npm-bootstrap-0.22.0` artifact naming.
Actual scoped publication, all eight trusted-publisher configurations, direct-publish
settings, and four-platform registry installs remain unchecked external work. This
does not start iteration 287 or iteration 288, and Homefinder remains deferred.

## Links

- [[research/npm-package-and-typed-typescript-api]] — proposal, naming table, settled section
- [[iterations/iteration-237-pi-package-distribution]] — the earlier package distribution and
  its version discipline (DEC-101)
