---
type: iteration
title: "Iteration 285 — Typed output structs and a typed JSON envelope"
date: 2026-09-06
status: planned
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

Two reasons to fix it, in this order: it is the "one concept in two code paths" defect class the
retrospective is expected to name (text vs JSON renderers drift because nothing pins the shape),
and it is the precondition for generating TypeScript types with `ts-rs` in
[[iterations/iteration-287-typed-typescript-api]]. This iteration is **Rust only** and changes
**no JSON byte**.

## Tasks

- [ ] TASK-1: `Envelope<T: Serialize>` with `results`, `total`, `hints`, the optional `dir`
      hoist and the three `files_*` counters as typed optional fields, replacing
      `build_envelope_value` and the post-hoc key injection. One `ErrorEnvelope` for the exit-1
      path (`error`, `cause`, `hints`). Serialization must be byte-identical to today, including
      key order and which keys are omitted vs `null`; the `results` key conventions in
      `rule-knowledgebase.md` are the spec.
- [ ] TASK-2: results structs for `read` and `summary` first (the first two API consumers), then
      `config`, `links` (all subcommands), `types`, `lint-rules`, `views`, `okf`, `madr`, `new`,
      `tasks`, `changelog`, `create-index`, `drop-index`. Every command's `results` is a named
      struct with `#[derive(Serialize)]` and doc comments on every field (they become JSDoc in
      287). Where a field is genuinely dynamic (frontmatter values) keep `serde_json::Value`
      and say so in the doc comment.
- [ ] TASK-3: parity guard: a test per command that runs the e2e fixture through the old
      renderer (kept behind a `#[cfg(test)]` shim for the duration of the branch, deleted at the
      end) and the new struct and asserts byte equality. Existing e2e snapshot tests must not
      change.
- [ ] TASK-4: fix the `pi-package` version-sync gate: `check-pi-package-sync` compares copies
      byte for byte but never checks `pi-package/package.json` (`0.1.1`) against the Cargo
      workspace version (`0.22.0`). Extend it to fail on mismatch; bump the package versions.
- [ ] TASK-5: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, every xtask `check-*` gate, `hyalo lint --strict` on the
      knowledgebase. DEC: "every `results` payload is a named struct; `json!` is not allowed in
      command output" — and an xtask or clippy-style grep gate that enforces it.

## Acceptance criteria

- [ ] `grep -rn 'json!(' crates/hyalo-cli/src/commands` returns zero hits outside tests.
- [ ] Every e2e JSON snapshot is unchanged; the parity test passes for every command.
- [ ] `hyalo config --jq`, `--files-from` counters and the `dir` hoist behave exactly as before.
- [ ] `check-pi-package-sync` fails when `pi-package/package.json` disagrees with Cargo.
- [ ] Gates green.

## Links

- [[research/npm-package-and-typed-typescript-api]] — "Precondition: typed output structs"
- [[iterations/iteration-284-stability-retrospective]] — expected to confirm the defect class
- [[iterations/iteration-287-typed-typescript-api]] — consumer of these structs
