---
type: iteration
title: "Iteration 286 — npm distribution: launcher and per-platform binary packages"
date: 2026-09-06
status: planned
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

`npm install hyalo` on any supported machine, with no Homebrew step and no download script: the
esbuild / Biome pattern from [[research/npm-package-and-typed-typescript-api]]. One main
package `hyalo` whose `bin` is a small JS launcher, plus one package per release target under
the `@ractive-ch` scope (org created 2026-09-06, owner `ractive.ch`, two-factor auth on),
selected by npm through `os` / `cpu` / `libc` in `optionalDependencies`. Needs **no
TypeScript types** — it is independent of 285 and 287.

Settled inputs (all in the research note): registry is npmjs.com with trusted publishing
(OIDC), no long-lived token; both glibc and musl Linux packages ship; Intel macOS is not built
and the launcher says so; version = Cargo workspace version, platforms published before the
main package, exact pins; `hoppy` and `ff-rdp` reuse the same scope and layout later.

## Tasks

- [ ] TASK-1: DEC weakening the "all code stays in Rust — no polyglot tooling" rule: it was
      written against Bun tests verifying Rust code; `npm/` and `pi-package/` may hold
      TypeScript because they *are* the JavaScript deliverable. Update `CLAUDE.md` in the same
      PR.
- [ ] TASK-2: `npm/hyalo/` — `package.json` (`bin`, `optionalDependencies` for the seven
      targets, `engines`, `files`), `bin/hyalo.js` launcher that resolves the platform package,
      execs the binary with inherited stdio and exit code, and fails with a clear message naming
      `os`/`cpu`/`libc` and pointing at `cargo install hyalo-cli` when no package matches (Intel
      macOS, unsupported libc). README with install and the platform table.
- [ ] TASK-3: `npm/platforms/` — a generator (an `xtask` subcommand, Rust) that writes one
      platform package per target from a template: name `@ractive-ch/hyalo-<os>-<cpu>[-musl]`,
      `os`/`cpu`/`libc` fields, the binary, the licence. Version taken from `Cargo.toml`.
- [ ] TASK-4: `release.yml` — after the archives exist, unpack each into its platform package,
      `npm publish --provenance` the seven platform packages, then the main package. Configure
      trusted publishing for each of the eight packages on npmjs.com (one-time, manual, James).
      A dry-run path (`npm pack` + `npm publish --dry-run`) runs on every PR touching `npm/` or
      the workflow.
- [ ] TASK-5: version gate: extend the `check-pi-package-sync` xtask (or the gate 285 adds) so
      `npm/hyalo/package.json`, every platform package and `pi-package/package.json` equal the
      Cargo workspace version.
- [ ] TASK-6: verify on real machines: `npm install hyalo` on macOS arm64, Linux x64 glibc,
      Linux x64 musl (Alpine container) and Windows x64; `npx hyalo --version` prints the
      release version. Record timings for a musl vs glibc `summary` on MDN if a musl host is at
      hand (the allocator question in the note).
- [ ] TASK-7: docs: README install section, `skill-hyalo.md`, the knowledgebase research note
      outcome. Gates: `cargo fmt`, clippy, `cargo test --workspace -q`, every xtask `check-*`,
      `hyalo lint --strict`.

## Acceptance criteria

- [ ] `npm install hyalo && npx hyalo --version` works on the four verified platforms with
      exactly one platform package installed.
- [ ] On an unsupported platform the launcher exits non-zero with the message naming the
      platform and the cargo fallback.
- [ ] A release publishes eight packages at the Cargo version, platforms first, with provenance.
- [ ] The version gate fails CI when any `package.json` disagrees with Cargo.
- [ ] The polyglot DEC is filed and `CLAUDE.md` reflects it.
- [ ] Gates green.

## Links

- [[research/npm-package-and-typed-typescript-api]] — proposal, naming table, settled section
- [[iterations/iteration-237-pi-package-distribution]] — the earlier package distribution and
  its version discipline (DEC-101)
