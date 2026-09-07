# @ractive-ch/hyalo

Hyalo is a command-line toolkit for structured Markdown knowledgebases. Install
the scoped package and run its `hyalo` executable with:

```sh
npm install @ractive-ch/hyalo
npx --no-install hyalo --version
```

npm selects one exact-version native package through `optionalDependencies`.
The launcher requires Node.js 22.14 or newer and npm 11.5.1 or newer.

| Rust target | npm package | os | cpu | libc |
| --- | --- | --- | --- | --- |
| `aarch64-apple-darwin` | `@ractive-ch/hyalo-darwin-arm64` | darwin | arm64 | — |
| `x86_64-unknown-linux-gnu` | `@ractive-ch/hyalo-linux-x64` | linux | x64 | glibc |
| `aarch64-unknown-linux-gnu` | `@ractive-ch/hyalo-linux-arm64` | linux | arm64 | glibc |
| `x86_64-unknown-linux-musl` | `@ractive-ch/hyalo-linux-x64-musl` | linux | x64 | musl |
| `aarch64-unknown-linux-musl` | `@ractive-ch/hyalo-linux-arm64-musl` | linux | arm64 | musl |
| `x86_64-pc-windows-msvc` | `@ractive-ch/hyalo-win32-x64` | win32 | x64 | — |
| `aarch64-pc-windows-msvc` | `@ractive-ch/hyalo-win32-arm64` | win32 | arm64 | — |

Intel macOS, unsupported CPU combinations, and Linux systems whose libc cannot
be identified receive an error naming `os`, `cpu`, and `libc`, with
`cargo install hyalo-cli` as the fallback. The launcher does not download a
binary or run an install script.

## Packaging and publication

`.github/workflows/npm-packages.yml` tests the launcher on Linux, macOS, and
Windows pull requests. Its packaging job stages all eight packages, checks their
tarball contents, dry-runs every publication, and runs the real host binary
through a temporary local install. Foreign-target fixture bytes in that job
verify package layout only.

After a version is public, the dispatch-only `.github/workflows/npm-registry.yml`
installs that exact version from the npm registry in fresh consumers and runs
the installed CLI on macOS arm64, Linux x64 glibc, Linux x64 musl, and Windows
x64. The musl check runs in a pinned Node Alpine image. This acceptance workflow
does not publish or change registry state.

`.github/workflows/release.yml` downloads the seven native archives produced
by the pinned reusable release workflow. By default, a manual workflow dispatch
builds the native archives, packs all eight npm packages, and dry-runs every
publication. The reusable workflow remains in dry-run mode for every manual
dispatch, so it does not publish crates or configured package-manager and Linux
repository releases.

A manual dispatch can publish only the npm packages by enabling `publish_npm`
and entering an `npm_version` that exactly matches the `hyalo-cli` Cargo
version. A missing or mismatched confirmation stops the job before npm
publication. A published release remains the broad release path: it validates
the release tag against the Cargo version, runs the configured reusable release
publishing, and publishes the npm packages.

The original `prepare_npm_bootstrap` mode remains available for a complete new
eight-package version. For the 0.22.0 scoped-main recovery, enable
`prepare_npm_main_bootstrap` and enter the same exact `npm_version`. The three
manual modes are mutually exclusive. Main-only preparation skips the reusable
native release job and all native artifact handling, verifies the canonical
metadata, packs `./npm/hyalo`, dry-runs that publication, signs its exact
tarball with the workflow's GitHub OIDC identity, and uploads
`npm-bootstrap-<version>`. It does not publish to npm or rebuild any of the
seven immutable platform packages.

Bootstrap signing deliberately calls the provenance generator and
`npm-package-arg` bundled inside the workflow's pinned npm 11.19.1. This is an
internal npm API, so an npm pin update must review the inline signing script
against that exact release. Signing creates public Sigstore provenance and a
transparency-log entry for the prepared tarball; it does not itself upload that
tarball to npm.

Normal npm publishing handles all seven platform packages first, followed by
`@ractive-ch/hyalo`, using npm provenance and GitHub OIDC without a repository
token fallback.
Before an immutable version is published, the workflow compares the local
tarball integrity with `dist.integrity` from the public npm registry. A retry
skips an identical existing artifact, publishes an explicitly missing version,
and stops on mismatched integrity or an inconclusive registry response.

For the 0.22.0 scoped-main recovery, the npm owner uses this runbook:

1. Run `prepare_npm_main_bootstrap` for the real Cargo version and
   download its `npm-bootstrap-<version>` artifact. Before uploading anything,
   verify that the main `.sigstore.json` bundle still matches the SHA-512 bytes
   of `ractive-ch-hyalo-<version>.tgz`, and verify the certificate identity names
   repository `ractive/hyalo`, workflow `.github/workflows/release.yml`, and the
   expected GitHub ref. Keep those reviewed tarball bytes unchanged.
2. Confirm the public `dist.integrity` of all seven immutable 0.22.0 platform
   packages still matches the previously verified artifacts. Do not rebuild or
   republish them. As the npm owner, bootstrap only the scoped main package:

   ```bash
   version=0.22.0 # replace with the confirmed Cargo version
   tarball="ractive-ch-hyalo-$version.tgz"
   npm publish "$tarball" \
     --provenance-file "${tarball%.tgz}.sigstore.json" \
     --access public --ignore-scripts --registry https://registry.npmjs.org
   ```

   `--provenance-file` verifies and attaches the prepared bundle. It is
   mutually exclusive with `--provenance`; do not pass both. Before retrying,
   inspect the scoped main version's `dist.integrity` and compare it with the
   preserved tarball. Skip only an immutable version whose integrity matches.
   Never try to overwrite a version or continue past a mismatch, missing proof,
   or another inconclusive registry response.

   GitHub OIDC authenticates the bootstrap workflow's Sigstore provenance
   generation. This attended owner upload authenticates to npm separately and
   therefore does not exercise npm trusted-publisher OIDC; a later workflow
   publication must verify that path.
3. Configure all eight existing packages' trusted publishers for repository
   `ractive/hyalo` and workflow `release.yml`.
4. Explicitly allow direct publishing for each configuration. New trusted
   publisher configurations may allow staged publishing only by default.
5. Verify `npm install @ractive-ch/hyalo` and
   `npx --no-install hyalo --version` on macOS arm64,
   Linux x64 glibc, Linux x64 musl, and Windows x64.

See npm's official [trusted publishing guide](https://docs.npmjs.com/trusted-publishers/)
and [`npm trust` requirements](https://docs.npmjs.com/cli/v11/commands/npm-trust/).
