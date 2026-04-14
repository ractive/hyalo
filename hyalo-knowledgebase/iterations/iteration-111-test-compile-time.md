---
title: "Reduce test compilation time"
type: iteration
date: 2026-04-14
status: planned
branch: iter-111/test-compile-time
tags: [testing, performance, developer-experience]
---

# iter-111: Reduce test compilation time

## Problem

`cargo test --workspace` takes 3+ minutes, but actual test execution is ~8 seconds. The bottleneck is compilation:

- **34 separate e2e test binaries** in `crates/hyalo-cli/tests/e2e_*.rs` — each one compiles and links independently against the full CLI
- **Doctest compilation**: 7.1s
- **Unit tests** (453 in hyalo-cli, 572 in hyalo-core): fast (<1.5s combined)

The slowest test suite at runtime is e2e_append (22 tests, 2.84s) — negligible compared to compile time.

## Options

### Option A: Single e2e test binary

Create `tests/e2e.rs` with `mod` includes for each file. All 34 files become modules in one binary → one link step instead of 34.

Pros: biggest compile-time win, simple change.
Cons: all-or-nothing — can't run a single test file with `cargo test --test e2e_find`.

### Option B: `cargo nextest`

Use `cargo nextest run` which parallelizes test binary execution better. Doesn't reduce compilation but may improve wall-clock time.

Pros: no code changes.
Cons: adds a dev dependency, doesn't fix root cause.

### Option C: Consolidate into fewer binaries

Group related e2e tests (e.g. all mutation tests in one binary, all query tests in another). Fewer binaries, still selectable.

Pros: balance between compile speed and selectability.
Cons: more work, arbitrary grouping.

## Tasks

- [ ] Benchmark current compile time vs test execution time
- [ ] Implement chosen option
- [ ] Verify no tests lost in consolidation
- [ ] Measure improvement
