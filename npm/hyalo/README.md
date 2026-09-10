# @ractive-ch/hyalo

Hyalo is a command-line toolkit for structured Markdown knowledgebases. Install
the scoped package and run its `hyalo` executable with:

```sh
npm install @ractive-ch/hyalo
npx --no-install hyalo --version
```

npm selects one exact-version native package through `optionalDependencies`.
The launcher requires Node.js 22.14 or newer and npm 11.5.1 or newer.

Version `0.23.0` adds the typed API below alongside the CLI launcher.
The earlier `0.22.0` package contains the CLI launcher only.

| Rust target | npm package | os | cpu | libc |
| --- | --- | --- | --- | --- |
| `aarch64-apple-darwin` | `@ractive-ch/hyalo-darwin-arm64` | darwin | arm64 | — |
| `x86_64-unknown-linux-gnu` | `@ractive-ch/hyalo-linux-x64` | linux | x64 | glibc |
| `aarch64-unknown-linux-gnu` | `@ractive-ch/hyalo-linux-arm64` | linux | arm64 | glibc |
| `x86_64-unknown-linux-musl` | `@ractive-ch/hyalo-linux-x64-musl` | linux | x64 | musl |
| `aarch64-unknown-linux-musl` | `@ractive-ch/hyalo-linux-arm64-musl` | linux | arm64 | musl |
| `x86_64-pc-windows-msvc` | `@ractive-ch/hyalo-win32-x64` | win32 | x64 | — |
| `aarch64-pc-windows-msvc` | `@ractive-ch/hyalo-win32-arm64` | win32 | arm64 | — |

## Typed API

ESM and CommonJS consumers can import the same API directly from the installed
package:

```ts
import { config, find, read, summary } from "@ractive-ch/hyalo";

const matches = await find({
  pattern: "error handling",
  properties: ["status=planned"],
  tag: ["project"],
  limit: 10,
});
const note = await read({ file: [matches.results[0].file] });
const vault = await summary({ recent: 5, depth: 1 });
const settings = await config();
```

`find`, `read`, `summary`, and `config` force `--format json --no-hints` and
return `Envelope<T>`. They reject `format`, `jq`, `count`, hint controls, and
filename-only projections at runtime because those flags would break the typed
result contract. Use `raw(argv)` when a command needs text, jq, or another
projection. `find` exposes the envelope's `total`, so callers do not need
`--count`.

By default the API resolves and spawns the installed platform binary directly,
without a shell. `binaryPath` selects an explicit Cargo/Homebrew/test binary;
`transport` injects another process runner. The Pi integration uses
`createPiTransport(pi)` so `pi.exec("hyalo", ...)` still finds a binary on
`PATH`. Pi reports killed children through the same timeout and abort error
classes. Its process API does not accept stdin, so the adapter rejects `stdin`
instead of running with an empty input. Every call also accepts `cwd`,
`timeoutMs`, and `AbortSignal`.

The Pi bundle projects only the vault directory and session-summary opt-in from
configuration. It accepts current config envelopes, pre-`[pi]` envelopes, and
the earlier flat config shape; legacy payloads keep linting enabled and default
session summaries to off. Generated ESM, CommonJS, and Pi JavaScript bundles
embed detect-libc's Apache-2.0 license and source notice.

Nonzero typed calls throw `HyaloError`, preserving `exitCode`, `stdout`,
`stderr`, and a parsed `ErrorEnvelope` when Hyalo emitted one. Plain exit-2
diagnostics remain plain stderr. Spawn, timeout, abort, invalid JSON, and empty
JSON failures have distinct error classes. `HyaloError.effects` retains per-path
committed/unchanged/failed/not-attempted states and index disposition;
`HyaloError.category` distinguishes mutation and output failures. Per-path failure
categories distinguish source conflicts, I/O and finalization. Inspect effects
before retrying, especially task toggles: a nonzero result can follow a committed
write. Successful public `set()` and `task()` retain their `ProcessResult` streams.

The bundled Pi runtime also exposes the internal `mutationReport()` accessor.
It executes once with JSON/no hints and returns actual effects alongside the
usual results. Its hidden CLI transport flag is not a public option or generated
argument field; ordinary success envelopes do not acquire this extra metadata.

Successful typed calls forward nonempty stderr to the caller's stderr by default.
To collect it instead, pass `onDiagnostics`:

```ts
const warnings: string[] = [];
const response = await find({
  pattern: "rust",
  index: true,
  onDiagnostics: (stderr) => { warnings.push(stderr); },
});
```

The callback receives the original stderr once and may return a promise, which is
awaited. A throw or rejection rejects the call unchanged after CLI success. Empty
stderr does not notify. `quiet` follows the CLI's suppression rules and exceptions.
Failed calls and invalid envelopes preserve stderr in their error objects without
reporting it again. Successful `set`, `task`, and `lint` calls also report while
retaining their stream results. `raw()` and `execute()` only return streams.
Pi renders collected typed-tool warnings as separate text alongside the result.

## Generated types

Rust owns the serialized contracts. Test-only `ts-rs` derives export declarations
to `src/generated`, while `src/types.ts` composes ergonomic partial argument
aliases and the summary `dir` projection. Refresh them with:

```sh
cargo run -p xtask -- generate-ts-types
```

CI runs `check-ts-types`, which regenerates into a temporary directory and
rejects changed, missing, or extra declarations without writing the checkout.
To add another typed command, extract its real clap `Args` struct if necessary,
add test-only `TS` derives to the argument/result graph, export it in each owning
crate's tests, regenerate, then implement and contract-test the wrapper. Do not
copy the Rust schema into a handwritten TypeScript interface.

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

The original `prepare_npm_bootstrap` mode remains available for preparing a
complete new eight-package version. Future releases should use the normal
trusted-publisher path above. The `prepare_npm_main_bootstrap` path was used
only for the completed 0.22.0 scoped-main recovery; current source contains
new API bytes and must not be used to recreate or upload that immutable
version.

Bootstrap signing deliberately calls the provenance generator and
`npm-package-arg` bundled inside the workflow's pinned npm 11.19.1. This is an
internal npm API, so an npm pin update must review the inline signing script
against that exact release. Signing creates public Sigstore provenance and a
transparency-log entry for the prepared tarball; it does not itself upload that
tarball to npm.

Normal npm publishing handles all seven platform packages first, followed by
`@ractive-ch/hyalo`, using npm provenance and GitHub OIDC without a repository
token fallback.
Publication dry runs use a fresh offline npm cache and disable OIDC only for
that process. They validate each local package contract independently of
immutable registry history. The real publication planner stays online: before
an immutable version is published, it compares the local tarball integrity with
`dist.integrity` from the public npm registry. A retry skips an identical
existing artifact, publishes an explicitly missing version, and stops on
mismatched integrity or an inconclusive registry response.

The 0.22.0 scoped-main recovery and all eight trusted-publisher configurations
are complete. Its exact source runbook remains available at commit `8c05111a`
for audit purposes. Do not repeat that recovery with the current tree or
publish different bytes under 0.22.0.

See npm's official [trusted publishing guide](https://docs.npmjs.com/trusted-publishers/)
and [`npm trust` requirements](https://docs.npmjs.com/cli/v11/commands/npm-trust/).
