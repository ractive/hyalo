---
title: "Iteration 307: JS API and Jev helper follow-ups"
type: iteration
date: 2026-10-04
tags: [iteration, npm, pi, jev, review]
status: completed
branch: iter-307/js-api-and-jev-fixes
---

# JS API and Jev helper follow-ups

## Problem

[[reviews/codebase-review-2026-10-03]] lists correctness gaps in the shipped
JavaScript deliverables ("P1 F4", "JavaScript deliverables") and in the Jev
helper and hyalo-tidy skill ("Jev helper and tidy skill"). A lint refusal reads
as a clean lint, the pi README states the wrong version floor, a timeout leaves
the child running, an undeclared policy type exits with the "unavailable" code,
and dead embedded copies of the tidy skill assets bloat the crate tarball.

## Tasks

### npm/hyalo and pi-package

- [x] F4: `lint()` throws `HyaloError` with the parsed envelope on exit 1 with empty stdout; `lintVaultFile` returns `unavailable` with the stderr text
- [x] pi README states hyalo >= 0.24; typed mutations on an older binary return a clear "hyalo too old" error
- [x] `pi-runtime.ts` parses the error envelope like every other typed call
- [x] Timeout/abort escalates SIGTERM to SIGKILL after a grace period, awaits `close` and detaches listeners
- [x] The pi extension lints the vault-relative path, not the absolute one
- [x] `pi-package/lib/hyalo-api.d.ts` re-exports the API types; `Awaited<ReturnType<…>>` workarounds removed
- [x] Typed `terms()`, `tags()` and `backlinks()` wrappers with tests and README rows
- [x] `HyaloSpawnError` re-wrap uses a brand check instead of `instanceof`

### npm/jev and the hyalo-tidy skill

- [x] An undeclared policy type is rejected up front as `Invalid` (exit 2) naming the type
- [x] A `.cmd`/`.bat` hyalo path is rejected with a clear `Invalid` message; jev.md requires a native executable
- [x] Oversized and empty sections defer with explicit reasons; jev.md states the budget
- [x] Provider response validation ignores unknown keys; test with an extra field
- [x] `TYPESAFE_` environment stripping is case-insensitive
- [x] Dead embedded hyalo-tidy copies deleted; mirror gates exclude them (DEC-344)
- [x] DEC-345 records the Jev integration; codex-integration.md describes the skill assets and receipt

## Acceptance Criteria

- [x] npm/hyalo: typecheck, build and tests pass against the release binary
- [x] npm/jev: typecheck, build and tests pass
- [x] xtask sync/check gates pass (jev assets, pi runtime, pi package sync, codex package, ts types, npm packages)
- [x] cargo fmt, clippy -D warnings, cargo test --workspace and `just gates` pass
- [x] `hyalo lint --strict` is clean on the changed knowledgebase files
