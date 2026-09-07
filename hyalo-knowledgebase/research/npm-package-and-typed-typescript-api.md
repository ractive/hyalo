---
title: hyalo on npm — per-platform binaries and a typed TypeScript API
type: research
date: 2026-09-06
status: active
origin: homefinder-eco-mcp session (2026-09-06); discussion between James and Claude about replacing that server's substring search with hyalo
related:
  - "[[iterations/iteration-236-typed-pi-tools]]"
  - "[[iterations/iteration-237-pi-package-distribution]]"
  - "[[decision-log]]"
tags:
  - research
  - npm
  - typescript
  - distribution
  - api
  - discussion
---

# hyalo on npm — per-platform binaries and a typed TypeScript API

## Why this came up

`homefinder-eco-mcp` (a TypeScript MCP server) ingests hand-written markdown
pages from several repos into `kb/docs/` and searches them with a
line-by-line `String.includes` scan: no ranking, no stemming, no snippets, a
hard cap of 60 hits in source order. The pages carry real frontmatter
(`type`, `owner`, `status`, `last_verified`, `sources`) and wikilinks, so
hyalo is the natural replacement. An unindexed `hyalo find` over the 48
pages takes 37 ms; the snapshot index is irrelevant at that size.

The question is not *whether* to call hyalo from TypeScript but *how* to
make that a first-class experience: `npm install @ractive-ch/hyalo`, `import { find }`,
typed results, no Homebrew step, no Dockerfile download.

## Proposal

### 1. Per-platform npm packages (the esbuild / Biome / Turbo pattern)

Standard, not a hack. npm's `os`, `cpu` and `libc` fields plus
`optionalDependencies` exist for exactly this. One main package `@ractive-ch/hyalo`
lists every platform package as an optional dependency; npm installs only
the one matching the machine. The main package's `bin` is a tiny JS
launcher that `require.resolve`s the platform package and spawns the
binary.

Every needed target already comes out of `release.yml`:

| Release archive                   | npm package                           | os / cpu / libc      |
| --------------------------------- | ------------------------------------- | -------------------- |
| aarch64-apple-darwin              | `@ractive-ch/hyalo-darwin-arm64`      | darwin / arm64       |
| x86_64-unknown-linux-musl         | `@ractive-ch/hyalo-linux-x64-musl`    | linux / x64 / musl   |
| aarch64-unknown-linux-musl        | `@ractive-ch/hyalo-linux-arm64-musl`  | linux / arm64 / musl |
| x86_64-unknown-linux-gnu          | `@ractive-ch/hyalo-linux-x64`         | linux / x64 / glibc  |
| aarch64-unknown-linux-gnu         | `@ractive-ch/hyalo-linux-arm64`       | linux / arm64 / glibc|
| x86_64-pc-windows-msvc            | `@ractive-ch/hyalo-win32-x64`         | win32 / x64          |
| aarch64-pc-windows-msvc           | `@ractive-ch/hyalo-win32-arm64`       | win32 / arm64        |

The scope is the author, not the product, so hoppy and ff-rdp reuse it:
main packages are unscoped where the name is free (`hyalo`, `ff-rdp`) and
scoped where it is not (`@ractive-ch/hoppy` — `hoppy` on npm is a datasets
package); every platform package is `@ractive-ch/<tool>-<os>-<cpu>`.

Settled in the session:

- **No `x86_64-apple-darwin`.** Cross-compiling it from an arm64 runner is
  possible in principle but cost a lot of time before; not worth it. The
  launcher fails with a clear message naming the platform and pointing at
  `cargo install hyalo-cli`.
- **Release order.** Platform packages publish first, the main package
  last, all with the identical version pinned exactly (no caret). Otherwise
  the main package is uninstallable for the minutes in between.
- **Version = Cargo workspace version.** Same tag, same number. The pi
  package claims the same discipline but nothing enforces it today (see
  "Settled": the version-sync gate has to be built).

### 2. A typed wrapper, living in this repo

`npm/hyalo/` holds the launcher and the API: `find`, `read`, `summary`,
later the mutations. Each builds argv, spawns the bundled binary with
`--format json --no-hints`, parses the envelope, and returns a typed value.
Spawn cost is ~30 ms per call, negligible against an MCP round trip.

Why here and not in the consumer: the wrapper's tests run against the
binary built from the same commit, so an envelope change breaks CI in the
PR that made it. A wrapper in a consumer repo only finds out after a
release. The existing `pi-package/extensions/hyalo.ts` hand-parses the same
envelopes and becomes the first consumer.

## Open question: hand-written types vs. generated types

**Decided 2026-09-06: generated, via `ts-rs`** (neither option as written —
see "Settled" below). The two options are kept for the record.

### Option A — hand-written TypeScript types + contract tests

- `npm/hyalo/src/types.ts` written by hand, mirroring the serde structs.
- A vitest suite runs every wrapped command against a fixture vault with the
  freshly built binary and validates the parsed JSON with zod schemas
  derived from the same types.
- **For:** no new Rust dependency, no build step, readable types with
  hand-written doc comments, can start today.
- **Against:** two sources of truth kept honest only by test coverage. A
  new optional field on an envelope is invisible until someone remembers
  to add it. Every `--fields` projection (`properties`, `sections`,
  `links`, `backlinks`, `properties_typed`) multiplies the surface; the
  find envelope alone already has a dozen conditional keys.

### Option B — generated types from the serde structs

- Add `schemars` derives to the envelope and payload structs and a
  `hyalo schema` subcommand (or an `xtask`) that emits JSON Schema per
  command.
- `npm/hyalo` generates `types.ts` from that at build time
  (`json-schema-to-typescript` or similar) and commits the output so
  consumers see a diff in review.
- **For:** zero drift by construction. The type of every field, every
  optional key, every enum (`kind: wikilink | embed | markdown | external |
  attachment`, `title_source`) comes from the Rust definition. Doc comments
  on the structs flow through as JSDoc. The schema doubles as
  machine-readable documentation for agents, which is the whole point of
  the hints system.
- **Against:** new dependency and derive on ~every output struct; some
  envelopes are built ad hoc as `serde_json::Value` today and would need
  real structs first (this is arguably a cleanup worth doing anyway).
  Generated TS from JSON Schema can be ugly for tagged unions unless the
  serde `tag`/`untagged` representation is chosen with generation in mind.
  Adds a build step that must run before `npm publish`.

### Things to check before deciding

- [x] How many CLI output paths still serialize `serde_json::Value` rather
      than a struct? Each is a blocker for B and an unverifiable hole in A.
- [x] Does `schemars` handle `IndexMap`, `serde-saphyr` values and the
      `properties` `{key: value}` map acceptably, or does the schema
      degrade to `object` there?
- [x] Is a hybrid worth it: generate the types (B) *and* keep the fixture
      contract tests (A)? The tests then guard runtime behaviour (exit
      codes, stderr, hint suppression) rather than shapes.

## Secondary open questions

- [x] **Linux: musl only?** No — ship glibc and musl, all four Linux
      packages as in the table (decided 2026-09-06). npm's `libc` field picks
      the right one; a glibc host gets the glibc build and its malloc, so
      musl's allocator only matters on Alpine-style hosts.
- [x] **npm scope.** Settled: `@ractive-ch/*`, see below.
- [x] **Snippets in ranked mode.** BM25 `find` returns files and scores
      only; per-line `matches` with section headings exist only in regex
      mode. A search tool that answers "these six files" forces a second
      call per file. Either the wrapper runs BM25 for ranking then a regex
      pass over the top N for snippets, or ranked mode grows a snippet
      field. The latter is the better fix and is independent of this note.
- [x] **A long-lived mode.** `hyalo serve --stdio` answering JSON-lines
      requests would remove the spawn per query and make the snapshot index
      meaningful for a server process. **Not for now** (decided 2026-09-06):
      the wrapper spawns per call; revisit only if a consumer measures the
      spawn cost as a problem.
- [x] **Consumer-side finding, not hyalo's bug:** the eco-mcp ingest
      prepends an HTML provenance comment *above* the `---` block, which
      hides the frontmatter from hyalo entirely (empty `properties` on
      every mapl-memory page). Fix on the consumer side; possibly worth a
      lint warning here ("frontmatter present but not at byte 0").

## Settled on 2026-09-06 (discussion after the note was written)

- **Registry: npmjs.com.** GitHub Packages needs a token for every read, even
  of public packages, and forces the `@ractive` scope. Rejected. James's npmjs
  account is `ractive.ch` with two-factor auth enabled (required for trusted
  publishing); use trusted publishing (OIDC) from the release workflow so no
  long-lived token is stored.
- **Scope: `@ractive-ch`**, one org for every ractive tool (hyalo, hoppy,
  ff-rdp), not one org per product. `@ractive` is held by an idle npm user
  that is not James (his account is `ractive.ch`) and npm does not transfer
  scopes, so it is gone; the user scope `@ractive.ch` would have worked but
  the dot reads like a domain. The `ractive-ch` org was created on 2026-09-06
  (free plan, owner `ractive.ch`). The original plan used an unscoped `hyalo`
  main package, but npm rejected the verified 0.22.0 upload as too similar to
  `yalc`. On 2026-09-07 the user authorized `@ractive-ch/hyalo` as the settled
  canonical main name. `ff-rdp` remains planned as unscoped where available;
  `hoppy` is taken, so hoppy's main package is scoped.
- **Types: generated with `ts-rs`**, not schemars + JSON Schema + a Node
  converter. One derive per output struct, `cargo test` writes the `.ts`
  files, `serde_json::Value` becomes `unknown`. No `hyalo schema` command.
  Contract tests against the freshly built binary stay (the hybrid).
- **Precondition:** typed output structs. Only `find` has one today; `read`,
  `summary`, `config`, `types`, `lint-rules`, `links`, `views`, `okf`, `madr`,
  `new`, `tasks` build `serde_json::Value` ad hoc, and the envelope itself is
  a `json!()` in `output.rs`. That cleanup is a Rust-only iteration and
  belongs to the stability retrospective, before any TS work.
- **Input side:** the clap structs already define commands and flags; export
  them with `ts-rs` too and build argv by hand in the wrapper (option 1). A
  JSON request mode over stdin (option 2, the `serve --stdio` idea) would
  reuse the same structs, but is explicitly **not planned** for now.
- **"No polyglot tooling" is weakened** for `npm/` and `pi-package/` — the
  rule targeted Bun tests verifying Rust code, which TS consumers of generated
  TS are not. Record as a DEC when the npm iteration starts.
- **Intel macOS stays out.** Cross-compiling from an arm64 runner is
  possible in principle, but it cost a lot of time before; not worth it.
- **Linux: glibc and musl both ship**, so the choice is npm's via the `libc`
  field, not ours. hyalo uses the system allocator and a musl build has never
  been timed (cross targets run no tests); that only affects musl hosts now.
  Optional later: time MDN on a musl build and add `mimalloc` for musl
  targets if it is clearly slower.
- **Version sync gate is missing:** `pi-package/package.json` is 0.1.1 while
  Cargo is 0.22.0 and `check-pi-package-sync` compares copies only. Fix the
  gate before adding a second npm package to the same promise.
- **Snippets in ranked mode** → [[iterations/iteration-283-ranked-search-snippets]].
- **Frontmatter-after-comment:** handled in the MCP server, nothing here.

## Iterations (filed 2026-09-06, to run after 0.22.0 is released)

Three separate iterations, not one, preceded by the stability retrospective
[[iterations/iteration-284-stability-retrospective]]:

1. **Typed output** — [[iterations/iteration-285-typed-output-structs]]
   (Rust only, retrospective work): `Envelope<T>` plus an
   error envelope struct replacing `build_envelope_value`; results structs for
   `read` and `summary` first, then the remaining `json!` commands. No
   behaviour change; JSON byte-identical, verified by the e2e suite.
2. **npm distribution** — [[iterations/iteration-286-npm-distribution]]
   (needs no types): `npm/hyalo/` launcher, platform
   package template under `npm/platforms/`, `release.yml` unpacking each
   archive into a platform package, publishing platforms then main via
   trusted publishing. Fix the version-sync gate for `pi-package/` in the
   same iteration. DEC for the polyglot rule.
3. **Typed API** — [[iterations/iteration-287-typed-typescript-api]]: `ts-rs` derives on the output and clap structs, generated
   `types.ts` committed, `find`/`read`/`summary` wrappers spawning the
   platform binary directly, vitest contract tests against the freshly built
   binary. Port `pi-package/extensions/hyalo.ts` onto it. Then switch
   `homefinder-eco-mcp` to `npm install @ractive-ch/hyalo`.

## Added 2026-09-07 — a generic markdown-KB MCP server?

From the follow-up discussion in the eco-mcp session (findings recorded in
that repo's `RUNTIME-KB-REFRESH.md`, section "2026-09-07"):

- The consumer expects mapl-memory to grow to **thousands of pages**. The
  eco-mcp ingests one product subtree; other product teams will want the
  same over theirs, and none of that is ecosystem-specific. That argues for
  a **generic markdown-KB MCP server**: repo + ref + paths in, hyalo for
  search, index and page reads, `create-index` after every pull, queries
  with `--index`. Since James owns hyalo, this is plausibly a hyalo feature
  (a `hyalo mcp` / `hyalo serve --mcp` subcommand, or a separate package on
  top of the npm wrapper from iteration 287) rather than a Comparis one.
  Not planned; decide before the consumer builds its ingest twice.
- Two consumer-side lessons that shape what such a server must do:
  never put page names into a tool-description enum (the eco-mcp does, and
  it does not survive thousands of pages); and derive the page list/titles
  from hyalo's index rather than opening every file.
- The consumer's provenance-comment bug is fixed on their side
  (comment now after the closing `---`). The possible lint rule
  "frontmatter present but not at byte 0" remains an idea here.

## Outcome — iteration 286 repository preparation (2026-09-07)

Iteration 285 completed the typed Rust output models and strengthened
`check-pi-package-sync` so the three existing package manifests must match
the Cargo workspace version. Iteration 286 extends that gate through one
canonical Rust npm platform table: the main manifest, all seven platform
manifests, exact optional-dependency pins, generated platform map, licenses,
and target READMEs are checked byte-for-byte apart from equivalent LF/CRLF line
endings. The generator also stages an
explicit seven-target binary input into a new output tree without deleting or
overwriting tracked sources.

The CommonJS launcher selects from the generated table, resolves the installed
optional package, and spawns the binary with inherited stdio, unchanged
arguments, numeric exit status, and signal propagation. Native `node:test`
coverage exercises all seven mappings, unsupported and missing-package errors,
and real child-process behavior.

Local packaging evidence on macOS arm64 used npm 11.19.1 to stage and pack all
eight packages, inspect every tarball allowlist, and run `npm publish --dry-run`
for each. A temporary consumer installed the local main and Darwin arm64
tarballs with optional registry packages omitted, then
`npx --no-install hyalo --version` ran the release binary and reported
`hyalo 0.22.0`. Foreign-target files were clearly labeled fixture bytes and
prove package layout only. The PR workflow repeats launcher tests on GitHub
Linux, macOS, and Windows runners and performs the packaging smoke with a real
Linux x64 glibc binary. These jobs passed on PR #336, together with all other
runnable checks at its reviewed head.

The release workflow is prepared to consume the seven exact archives produced
by `ractive/release-workflows/.github/workflows/release.yml@v0.2.0`, dry-run
all eight packages on default manual dispatch, and publish platform packages before
the main package for a published release. An explicit npm-only manual option uses
`publish_npm=true` and an `npm_version` matching Cargo. The reusable workflow stays
in dry-run mode for every manual dispatch, so native archives are built without
publishing to crates.io, Homebrew, Scoop, winget, AUR or Cloudsmith. Only the npm
publication step opts in. No workflow was dispatched and no package was published
during this preparation.

Publication retries are fail-closed: a Rust gate compares each local
`npm pack` integrity with the exact version's `dist.integrity` at
`https://registry.npmjs.org`. An explicit missing-version 404 remains
publishable, an identical immutable artifact is skipped, and mismatched
integrity, authorization, network, or malformed responses stop the job before
publication. This allows a partial platform-package success to resume without
attempting to overwrite earlier immutable versions, while keeping the main
package last.

External acceptance remains open. The owner must bootstrap all eight packages,
configure each existing package's trusted publisher for `ractive/hyalo` and
`release.yml`, explicitly allow direct publishing where new configurations
default to staged publishing, authorize a release, and verify public-registry
installs on macOS arm64, Linux x64 glibc, Linux x64 musl, and Windows x64. The
2026-09-07 public metadata requests returned 404 for all eight names; that
shows only that no public metadata existed, not ownership or private-package
state.

Authoritative references:

- npm trusted publishing:
  <https://docs.npmjs.com/trusted-publishers/>
- npm 11 `npm trust` prerequisites:
  <https://docs.npmjs.com/cli/v11/commands/npm-trust/>
- Reusable release workflow source:
  `ractive/release-workflows/.github/workflows/release.yml@v0.2.0`,
  Git blob `300705e94fa0090441861a653ccd7f05c36a749a`

## Outcome — scoped main recovery (2026-09-07)

All seven `@ractive-ch/hyalo-<platform>` packages at 0.22.0 were published
from the verified native artifacts before npm rejected the unscoped main name.
Their versions and bytes are immutable. The canonical main registry package is
now `@ractive-ch/hyalo`; its executable remains `hyalo`, and its exact platform
dependency names and 0.22.0 pins are unchanged.

The repository recovery prepares only `ractive-ch-hyalo-0.22.0.tgz` from the
tracked main-package source, without a native build. Publication, all eight
GitHub trusted-publisher relationships with direct publishing, and the four
real-platform registry installs remain external acceptance work. No external
check was treated as completed by this repository change. Iteration 287 still
depends on full iteration 286 completion, while Homefinder and iteration 288
remain deferred.
