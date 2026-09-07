# hyalo

This package is prepared for publication but is not yet available from the
public npm registry. Until owner bootstrap, trusted publishing, and registry
installation checks are complete, install the CLI with:

```sh
cargo install hyalo-cli
```

After public acceptance, the intended npm installation will be:

```sh
npm install hyalo
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

For the initial owner upload, enable `prepare_npm_bootstrap` instead and enter
the same exact `npm_version`. That option and `publish_npm` are mutually
exclusive. The job builds and packs the same artifacts, then signs each final
tarball with the workflow's GitHub OIDC identity and uploads
`npm-bootstrap-<version>`. It does not publish to npm and needs no npm registry
credential. The default manual dispatch remains the unchanged dry run.

Bootstrap signing deliberately calls the provenance generator and
`npm-package-arg` bundled inside the workflow's pinned npm 11.19.1. This is an
internal npm API, so an npm pin update must review the inline signing script
against that exact release. Signing creates public Sigstore provenance and a
transparency-log entry even though the npm packages remain unpublished.

Both npm publishing paths use the same staged tarballs and publication plan:
all seven platform packages are handled first, followed by `hyalo`, using npm
provenance and GitHub OIDC without a repository token fallback.
Before an immutable version is published, the workflow compares the local
tarball integrity with `dist.integrity` from the public npm registry. A retry
skips an identical existing artifact, publishes an explicitly missing version,
and stops on mismatched integrity or an inconclusive registry response.

The npm owner must complete these external steps before the first registry
release:

1. Run the bootstrap-preparation dispatch for the real Cargo version and
   download its `npm-bootstrap-<version>` artifact. Before uploading anything,
   verify that every `.sigstore.json` bundle still matches the SHA-512 bytes of
   its adjacent `.tgz`, and verify the Sigstore certificate identity names
   repository `ractive/hyalo`, workflow `.github/workflows/release.yml`, and the
   expected GitHub ref. Keep those reviewed tarball bytes unchanged.
2. As the npm owner, bootstrap the unscoped `hyalo` package and all seven scoped
   packages under `@ractive-ch` in platform-first order, followed by `hyalo`:

   ```bash
   (
     set -eu
     version=0.22.0 # replace with the confirmed Cargo version
     for tarball in \
       ractive-ch-hyalo-{darwin-arm64,linux-x64,linux-arm64,linux-x64-musl,linux-arm64-musl,win32-x64,win32-arm64}-"$version".tgz \
       hyalo-"$version".tgz; do
       npm publish "$tarball" \
         --provenance-file "${tarball%.tgz}.sigstore.json" \
         --access public --ignore-scripts --registry https://registry.npmjs.org
     done
   )
   ```

   `--provenance-file` verifies and attaches the prepared bundle. It is
   mutually exclusive with `--provenance`; do not pass both. All eight packages
   currently lack public registry metadata. The subshell stops at the first
   failed upload, so `hyalo` cannot publish after a platform-package failure.
   Before retrying, inspect the registry's `dist.integrity` for every attempted
   package and compare it with the preserved tarball. Skip only an immutable
   version whose integrity matches exactly, then resume in the same order.
   Never try to overwrite a version or continue past a mismatch, missing proof,
   or another inconclusive registry response.
3. Configure each existing package's trusted publisher for repository
   `ractive/hyalo` and workflow `release.yml`.
4. Explicitly allow direct publishing for each configuration. New trusted
   publisher configurations may allow staged publishing only by default.
5. Publish an authorized release through the OIDC workflow, then verify a
   registry install and `npx --no-install hyalo --version` on macOS arm64,
   Linux x64 glibc, Linux x64 musl, and Windows x64.

See npm's official [trusted publishing guide](https://docs.npmjs.com/trusted-publishers/)
and [`npm trust` requirements](https://docs.npmjs.com/cli/v11/commands/npm-trust/).
