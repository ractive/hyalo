---
type: iteration
title: "Iteration 285 — Typed output structs and a typed JSON envelope"
date: 2026-09-06
status: completed
tags: [iteration, output, json, architecture]
branch: iter-285/typed-output-structs
priority: 2
depends-on: "[[iterations/iteration-284-stability-retrospective]]"
related:
  - "[[research/npm-package-and-typed-typescript-api]]"
  - "[[iterations/iteration-287-typed-typescript-api]]"
  - "[[decision-log]]"
---

# Iteration 285 — Typed output structs and a typed JSON envelope

## Goal

The JSON envelope is built by `build_envelope_value` in `crates/hyalo-cli/src/output.rs:226`
with a `json!()` macro, and `output_pipeline.rs:59-77` injects `files_missing` /
`files_skipped_non_md` / `files_skipped_outside_vault` as sibling keys afterwards. About half
the commands serialize a real struct for `results` (`find` via `FileObject`, `lint`, `mv`,
`set`, `remove`, `tags`, `properties`, `backlinks`); the other half build `serde_json::Value`
ad hoc: `read`, `summary`, `config`, `types` (20 `json!` sites), `lint-rules` (15), `links` (7),
`views`, `okf`, `madr`, `new`, `tasks`, `changelog`, `create-index`, `drop-index`.

Two reasons to fix it: explicit output-family contracts can guard shape drift, and typed
results are the precondition for generating TypeScript types with `ts-rs` in
[[iterations/iteration-287-typed-typescript-api]]. This iteration is **Rust only** and changes
**no JSON byte**.

[[research/stability-retrospective-2026-09-06]] supports output-contract and parity guards,
while showing that existing named structs also drifted and cannot force missing planning
to happen. Keep byte-parity tests and shared family contracts; deriving Serialize alone
does not establish semantic correctness or explain every output/help defect.

## Tasks

- [x] TASK-1: `Envelope<T: Serialize>` with `results`, `total`, `hints`, the optional `dir`
      hoist and the three `files_*` counters as typed optional fields, replacing
      `build_envelope_value` and the post-hoc key injection. One `ErrorEnvelope` for the exit-1
      path (`error`, optional `path`, `hint`, and `cause`; preserve the existing singular
      `hint` key). Preserve specialized structured error payloads as well. Serialization must
      be byte-identical to today, including
      key order and which keys are omitted vs `null`; the `results` key conventions in
      `crates/hyalo-cli/templates/rule-knowledgebase.md` are the spec.
- [x] TASK-2: results structs for `read` and `summary` first (the first two API consumers), then
      `config`, `links` (all subcommands), `types`, `lint-rules`, `views`, `okf`, `madr`, `new`,
      `tasks`, `changelog`, `create-index`, `drop-index`. Every command's `results` is a named
      struct with `#[derive(Serialize)]` and doc comments on every field (they become JSDoc in
      287). Where a field is genuinely dynamic (frontmatter values) keep `serde_json::Value`
      and say so in the doc comment.
- [x] TASK-3: parity guard: a test per command that runs the e2e fixture through the old
      renderer (kept behind a `#[cfg(test)]` shim for the duration of the branch, deleted at the
      end) and the new struct and asserts byte equality. Existing e2e snapshot tests must not
      change.
- [x] TASK-4: fix the `pi-package` version-sync gate: `check-pi-package-sync` compares copies
      byte for byte but never checks `pi-package/package.json` (`0.1.1`) against the Cargo
      workspace version (`0.22.0`). Extend it to fail on mismatch; bump the package versions.
- [x] TASK-5: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, every xtask `check-*` gate, `hyalo lint --strict` on the
      knowledgebase. DEC: "every `results` payload is a named struct; `json!` is not allowed in
      command output" — and an xtask or clippy-style grep gate that enforces it.

## Acceptance criteria

- [x] `grep -rn 'json!(' crates/hyalo-cli/src/commands` returns zero hits outside tests.
- [x] Every e2e JSON snapshot is unchanged; the parity test passes for every command.
- [x] `hyalo config --jq`, `--files-from` counters and the `dir` hoist behave exactly as before.
- [x] `check-pi-package-sync` fails when `pi-package/package.json` disagrees with Cargo.
- [x] Gates green.

## Plan reconciliation — 2026-09-06, iteration 284

Baseline inspection of `output::format_error` found a singular optional `hint` and an
optional `path`, rather than the proposed `hints` field. Corrected TASK-1 to preserve the
existing wire contract, including specialized budget-error fields. The completed retrospective
also replaces the goal's anticipated causal claim with its evidence-backed limitations.
The byte-parity outcome and acceptance strength are unchanged.

## Plan reconciliation — 2026-09-07, iteration 283

The byte-parity baseline now includes ranked `FileObject.matches` arrays (including empty
arrays for title-only or cross-line phrase hits) and the shared authored-title scoring rule.
Preserve these values and the repaired Unicode/frontmatter boundaries during the output
refactor. The existing `ContentMatch` shape is unchanged; no additional output type is needed.

## Outcome — 2026-09-07

DEC-331 establishes named output contracts and the `check-typed-output` production
macro gate. `summary` and `tasks` already had named contracts; those were retained and
documented rather than replaced. The envelope includes typed optional counters and
preserves singular error `hint`, optional `path`/`cause`, specialized budget errors,
omissions, nulls, directory hoisting and alphabetical JSON keys. Existing JSON values
are formatted by reference; typed values are normalized once.

All 35 temporary old-renderer assertion sites executed through 19 fixture drivers;
the shims were archived and removed. All 64 complete baseline/candidate CLI comparisons
matched stdout, stderr and exit status, including jq, file counters, errors and ranked
snippets. Existing e2e sources and snapshots are unchanged. Final validation passed:
4,882 tests, zero failures, two existing doctest ignores; fmt, clippy and all 11 xtask
checks (including two pre-existing stubs). Full strict knowledgebase lint retained four
existing HYALO002 warnings; changed-file lint was clean. The changelog profile retained
its existing HYALO006 warning. Independent review found two redundant result copies;
both were repaired and the fresh full review was clean.

All three pi manifests now match Cargo 0.22.0, with a negative version-drift check.
This does not publish any package or complete the external prerequisites of 286/287.

## Links

- [[research/npm-package-and-typed-typescript-api]] — "Precondition: typed output structs"
- [[iterations/iteration-284-stability-retrospective]] — completed defect-class evidence
- [[iterations/iteration-287-typed-typescript-api]] — consumer of these structs
