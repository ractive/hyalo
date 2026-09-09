---
type: iteration
title: "Iteration 287 — Typed TypeScript API generated with ts-rs"
date: 2026-09-06
status: in-progress
tags: [iteration, npm, typescript, api]
branch: iter-287/typed-typescript-api
priority: 3
depends-on: "[[iterations/iteration-286-npm-distribution]]"
related:
  - "[[iterations/iteration-285-typed-output-structs]]"
  - "[[research/npm-package-and-typed-typescript-api]]"
  - "[[iterations/iteration-236-typed-pi-tools]]"
  - "[[decision-log]]"
---

# Iteration 287 — Typed TypeScript API generated with ts-rs

## Goal

`import { find, read, summary } from "@ractive-ch/hyalo"` with result types that come from the Rust
structs, not from a hand-written mirror. Decided in
[[research/npm-package-and-typed-typescript-api]]: `ts-rs` derives on the output structs from
[[iterations/iteration-285-typed-output-structs]] and on the clap argument structs;
`cargo run -p xtask -- generate-ts-types` writes the declarations and public barrels, which are
committed so a change shows up in review. No schemars, no JSON
Schema, no `hyalo schema` command, no stdio server mode. Contract tests run against the binary
built from the same commit. Requires both 285 (structs) and 286 (the package to ship in).

## Tasks

- [x] TASK-1: `ts-rs` (features `serde-json-impl`, `indexmap-impl`) as a dev-dependency;
      `#[derive(TS)]` + `#[ts(export)]` on `Envelope<T>`, `ErrorEnvelope`, `FileObject` and its
      parts (`ContentMatch`, `PropertyInfo`, links, sections, tasks, backlinks), the `read` and
      `summary` results, and the `FindArgs` / `ReadArgs` / `SummaryArgs` clap structs plus the
      global args struct. Doc comments flow through as JSDoc. `serde_json::Value` fields export
      as `unknown`.
- [x] TASK-2: an xtask gate `check-ts-types` that regenerates into a temp dir and diffs against
      the committed `npm/hyalo/src/generated/*.ts`; fails on drift. Runs in `quality-gates.yml`.
- [x] TASK-3: `npm/hyalo/src/` — `find(args)`, `read(args)`, `summary(args)`: build argv from
      the generated arg type (global flags from the global struct; custom-parsed values such as
      `--property K=V` are typed as the string form and documented), spawn the **platform
      binary directly** (not the launcher) with `--format json --no-hints`, parse into
      `Envelope<T>`, map exit 1 to a thrown `HyaloError` carrying `ErrorEnvelope`, and preserve
      exit 2 as usage **or internal** failure with the original stderr (which may be plain
      text). Clap `conflicts_with` / `requires` rules are documented on the type, not
      enforced.
- [x] TASK-4: vitest contract tests against a fixture vault and the freshly built binary:
      shapes match the generated types (a runtime check generated from the same structs, or
      `expectTypeOf` plus a JSON round-trip), exit codes, stderr passthrough, hint suppression,
      the `files_missing` counters, an unparsable-frontmatter `--file` (DEC-301). CI builds the
      binary once and points the tests at it.
- [x] TASK-5: port `pi-package/extensions/hyalo.ts` (485 lines, typebox schemas, hand-parsed
      `config` JSON) onto the wrapper; typed tools keep their typebox parameter schemas but stop
      hand-building argv and parsing JSON.
- [x] TASK-6: docs: API README with the three calls, the generated-types workflow, how to add a
      command; `skill-hyalo.md` note; research note outcome. Gates: `cargo fmt`, clippy,
      `cargo test --workspace -q`, every xtask `check-*`, `npm test`, `hyalo lint --strict`.
- [ ] TASK-7 (consumer, outside this repo): switch `homefinder-eco-mcp` to `npm install @ractive-ch/hyalo`
      and the typed `find`; record what the wrapper lacked.
      Deferred by the user on 2026-09-07 to concentrate on Hyalo; retain this requirement
      as unfinished and do not modify that repository during the current batch.

## npm publication — 2026-09-08

Published `@ractive-ch/hyalo@0.23.0` and all seven matching platform packages at
the user's request, before Homefinder acceptance. Release preparation landed in
[PR #344](https://github.com/ractive/hyalo/pull/344); [PR #345](https://github.com/ractive/hyalo/pull/345)
removed the optional type formatter's unmaintained dependency without changing
the generated API types or relaxing security policy. Both PRs passed independent
review and all eleven CI checks.

[Publication run #34199953078](https://github.com/ractive/hyalo/actions/runs/34199953078)
succeeded from commit `73769bd76720669ba0688aaeff58b7c00479807e` using the npm-only
workflow. All eight public tarballs match their registry SHA-512 integrity values
and expose provenance metadata. A fresh macOS consumer verified the published
CLI, ESM/CommonJS `find`, `read`, `summary`, and `config`, TypeScript consumption,
and registry signatures and attestations for its three installed packages.

[Registry smoke run #34201309749](https://github.com/ractive/hyalo/actions/runs/34201309749)
passed on macOS arm64, Windows x64, Linux x64 glibc, and Linux x64 musl. The glibc
tarball initially returned 404 after publication; it became available with the
expected integrity, and the failed smoke job passed on retry without republishing.
TASK-7 remains open: publication does not establish Homefinder consumer acceptance.

## Remaining work — 2026-09-09

TASK-7 is the only unfinished task in this iteration. The Homefinder migration
remains deferred; resume it in `homefinder-eco-mcp`, verify the published package's
typed `find` against that consumer, and record any API gaps here before marking
this iteration completed. Repository implementation and npm publication are done.

The desktop verification follow-up in
[[iterations/iteration-288-codex-integration]] is completed on the user's manual
confirmation, with the evidence limits recorded in that iteration.

## Acceptance criteria

- [x] `types.ts` is generated, committed, and `check-ts-types` fails on drift.
- [x] `find`, `read`, `summary` return values whose TypeScript type is the exported Rust
      struct, verified by the contract suite against the same-commit binary.
- [x] Error paths surface `ErrorEnvelope` with the original exit code.
- [x] The pi extension no longer hand-parses any hyalo JSON.
- [x] Gates green, including `npm test` in CI.

## Plan reconciliation — 2026-09-06, iteration 284

Baseline inspection of `run.rs` and `output_pipeline.rs` confirms DEC-307 reserves exit 2
for both clap usage errors and internal failures. TASK-3 now preserves that distinction
and raw stderr instead of mislabeling every exit-2 failure as usage. Error-path coverage
remains required. Both 285 and the full 286 prerequisite remain required; npm publication,
real-platform verification, and the external consumer migration are not completed by
repository-only work in this batch.

## Plan reconciliation — 2026-09-07, iteration 283

Ranked `find` now returns up to three existing `ContentMatch` objects per result, ordered by
distinct-token coverage then line number; title-only and cross-line phrase hits can have an
empty array. Include these cases in the typed `find` contract fixtures. The generated shape
and the full 285/286 prerequisites remain unchanged.

## Plan reconciliation — 2026-09-07, iteration 285

The named output contracts and typed envelope are implemented. The Rust envelope has a
borrowed lifetime (`Envelope<'a, T>`); its pipeline adapter borrows existing JSON values
and copies only when removing the hoisted directory. Keep generated TypeScript focused
on the serialized contract. With `ts-rs` as a dev-dependency, gate its derives/exports
to the test build so normal production builds do not require that dependency. Preserve
the documented dynamic JSON fields as `unknown`, optional omissions and singular error
`hint`. Existing summary/task models were retained. Both the full 286 prerequisite and
the external consumer migration remain unfinished; this reconciliation does not start
the TypeScript implementation.

## Plan reconciliation — 2026-09-07, iteration 286

The repository-preparation branch introduces a CommonJS launcher and an internal
binary resolver at `npm/hyalo/lib/resolve-platform.js`. TASK-3 should share that
resolver while spawning the native binary directly. The Rust npm generator owns
`npm/hyalo/package.json`; adding API exports, type entrypoints or build metadata
must update the generator and its tests as well as the generated manifest.

Iteration 286's repository preparation has passed fresh review, including its
CRLF metadata-check fix. npm bootstrap, trusted publishing, actual publication and
four-platform registry installation checks remain outstanding. Both full prerequisites remain
required; no TypeScript implementation has started. TASK-7 still requires separate
authority to modify `homefinder-eco-mcp` and is not waived by repository-only work.

## Plan reconciliation — 2026-09-07, scoped main recovery

The canonical registry spec is now `@ractive-ch/hyalo`; the installed executable,
source directory `npm/hyalo`, and direct native-binary resolver stay unchanged.
TASK-2 and TASK-3 must generate exports and build metadata into that scoped package.
They must also extend the main-only bootstrap package-content check when API files
are added to the manifest allowlist. Iteration 287 remains blocked until scoped
publication, trust configuration, and four-platform installation acceptance finish.
Homefinder and iteration 288 remain deferred.

## Plan reconciliation — 2026-09-07, final npm publication status

All eight 0.22.0 packages are public with verified integrity, and registry installs
now pass on the four required platforms. Trusted publishers for all eight packages
are configured for `ractive/hyalo` and `release.yml` with direct publishing enabled,
so iteration 287's full iteration-286 prerequisite is fulfilled. No iteration 287
implementation has started; Homefinder TASK-7 and iteration 288 desktop verification
remain deferred.

## Plan reconciliation — 2026-09-07, implementation

The inline clap fields were extracted into the real `FindArgs`, `ReadArgs`, and
`SummaryArgs` structs without changing their parsers, conflicts, defaults, or
long help. Test-only derives keep `ts-rs` out of production dependency graphs.
Generation is explicit through `generate-ts-types`; `check-ts-types` is a
temporary, read-only freshness gate. The committed inventory is 40 declarations,
including its generated barrel, plus the public `types.ts` barrel.

The API adds the required `config()` wrapper because Pi's cached configuration
lookup was its remaining JSON parse. Typed calls own JSON/no-hints output and
reject incompatible transforms; `raw` serves generic text and slash-command
paths. Native child execution handles early stdin closure, timeout, abort, spawn
failure, plain exit-2 stderr, and parsed exit-1 envelopes distinctly. Structured
values use literal-safe option encoding and positional terminators so an
option-looking path cannot broaden a mutation. The Pi adapter retains PATH
resolution through `pi.exec` rather than forcing npm's platform resolver.

Pi uses generated `pi-package/lib/hyalo-api.js` and the bundled canonical
declarations in `hyalo-api.d.ts`. Keeping helpers under `lib` prevents Pi from
auto-loading the runtime as a second extension. Both root package installs and
offline `init --pi` layouts import the same artifacts; sync, embed, install,
deinit, static type checking, and freshness checks cover them. Public npm 0.22.0
remains unchanged and CLI-only. Homefinder TASK-7 and iteration 288's desktop
verification remain deferred.

## Hyalo delivery — 2026-09-07

PR #342 implements TASK-1 through TASK-6. Fresh independent reviews and all eleven
applicable CI checks passed at `88bb2a7f`, including the same-source API tests on
Linux, macOS, and Windows. The Windows early-stdin-close regression is covered.
Homefinder TASK-7 remains deferred, so this iteration remains in progress. The API
has not been published; public npm 0.22.0 remains CLI-only.

## Links

- [[research/npm-package-and-typed-typescript-api]] — decision record
- [[iterations/iteration-285-typed-output-structs]] — the structs being exported
- [[iterations/iteration-286-npm-distribution]] — the package this ships in
- [[iterations/iteration-236-typed-pi-tools]] — the pi tools being ported
