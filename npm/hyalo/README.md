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

`.github/workflows/release.yml` downloads the seven native archives produced
by the pinned reusable release workflow. A manual workflow dispatch only packs
and dry-runs the packages. A published release event is the only path that can
publish: all seven platform tarballs publish first, followed by `hyalo`, using
npm provenance and GitHub OIDC without a repository token fallback.
Before an immutable version is published, the workflow compares the local
tarball integrity with `dist.integrity` from the public npm registry. A retry
skips an identical existing artifact, publishes an explicitly missing version,
and stops on mismatched integrity or an inconclusive registry response.

The npm owner must complete these external steps before the first registry
release:

1. Bootstrap the unscoped `hyalo` package and all seven scoped packages under
   `@ractive-ch`; all eight currently lack public registry metadata.
2. Configure each existing package's trusted publisher for repository
   `ractive/hyalo` and workflow `release.yml`.
3. Explicitly allow direct publishing for each configuration. New trusted
   publisher configurations may allow staged publishing only by default.
4. Publish an authorized release, then verify a registry install and
   `npx --no-install hyalo --version` on macOS arm64, Linux x64 glibc,
   Linux x64 musl, and Windows x64.

See npm's official [trusted publishing guide](https://docs.npmjs.com/trusted-publishers/)
and [`npm trust` requirements](https://docs.npmjs.com/cli/v11/commands/npm-trust/).
