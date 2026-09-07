---
type: iteration
title: "Iteration 287 — Typed TypeScript API generated with ts-rs"
date: 2026-09-06
status: planned
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

`import { find, read, summary } from "hyalo"` with result types that come from the Rust
structs, not from a hand-written mirror. Decided in
[[research/npm-package-and-typed-typescript-api]]: `ts-rs` derives on the output structs from
[[iterations/iteration-285-typed-output-structs]] and on the clap argument structs; `cargo test`
writes `types.ts`, which is committed so a change shows up in review. No schemars, no JSON
Schema, no `hyalo schema` command, no stdio server mode. Contract tests run against the binary
built from the same commit. Requires both 285 (structs) and 286 (the package to ship in).

## Tasks

- [ ] TASK-1: `ts-rs` (features `serde-json-impl`, `indexmap-impl`) as a dev-dependency;
      `#[derive(TS)]` + `#[ts(export)]` on `Envelope<T>`, `ErrorEnvelope`, `FileObject` and its
      parts (`ContentMatch`, `PropertyInfo`, links, sections, tasks, backlinks), the `read` and
      `summary` results, and the `FindArgs` / `ReadArgs` / `SummaryArgs` clap structs plus the
      global args struct. Doc comments flow through as JSDoc. `serde_json::Value` fields export
      as `unknown`.
- [ ] TASK-2: an xtask gate `check-ts-types` that regenerates into a temp dir and diffs against
      the committed `npm/hyalo/src/generated/*.ts`; fails on drift. Runs in `quality-gates.yml`.
- [ ] TASK-3: `npm/hyalo/src/` — `find(args)`, `read(args)`, `summary(args)`: build argv from
      the generated arg type (global flags from the global struct; custom-parsed values such as
      `--property K=V` are typed as the string form and documented), spawn the **platform
      binary directly** (not the launcher) with `--format json --no-hints`, parse into
      `Envelope<T>`, map exit 1 to a thrown `HyaloError` carrying `ErrorEnvelope`, and preserve
      exit 2 as usage **or internal** failure with the original stderr (which may be plain
      text). Clap `conflicts_with` / `requires` rules are documented on the type, not
      enforced.
- [ ] TASK-4: vitest contract tests against a fixture vault and the freshly built binary:
      shapes match the generated types (a runtime check generated from the same structs, or
      `expectTypeOf` plus a JSON round-trip), exit codes, stderr passthrough, hint suppression,
      the `files_missing` counters, an unparsable-frontmatter `--file` (DEC-301). CI builds the
      binary once and points the tests at it.
- [ ] TASK-5: port `pi-package/extensions/hyalo.ts` (485 lines, typebox schemas, hand-parsed
      `config` JSON) onto the wrapper; typed tools keep their typebox parameter schemas but stop
      hand-building argv and parsing JSON.
- [ ] TASK-6: docs: API README with the three calls, the generated-types workflow, how to add a
      command; `skill-hyalo.md` note; research note outcome. Gates: `cargo fmt`, clippy,
      `cargo test --workspace -q`, every xtask `check-*`, `npm test`, `hyalo lint --strict`.
- [ ] TASK-7 (consumer, outside this repo): switch `homefinder-eco-mcp` to `npm install hyalo`
      and the typed `find`; record what the wrapper lacked.

## Acceptance criteria

- [ ] `types.ts` is generated, committed, and `check-ts-types` fails on drift.
- [ ] `find`, `read`, `summary` return values whose TypeScript type is the exported Rust
      struct, verified by the contract suite against the same-commit binary.
- [ ] Error paths surface `ErrorEnvelope` with the original exit code.
- [ ] The pi extension no longer hand-parses any hyalo JSON.
- [ ] Gates green, including `npm test` in CI.

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

## Links

- [[research/npm-package-and-typed-typescript-api]] — decision record
- [[iterations/iteration-285-typed-output-structs]] — the structs being exported
- [[iterations/iteration-286-npm-distribution]] — the package this ships in
- [[iterations/iteration-236-typed-pi-tools]] — the pi tools being ported
