---
title: Reduce test compilation time
type: iteration
date: 2026-04-14
status: completed
branch: iter-111/test-compile-time
tags:
  - testing
  - performance
  - developer-experience
---

# iter-111: Reduce test compilation time

## Problem

`cargo test --workspace` takes 3+ minutes, but actual test execution is ~8 seconds. The bottleneck is compilation:

- **31 separate e2e test binaries** in `crates/hyalo-cli/tests/e2e_*.rs` — each one compiles and links independently against the full CLI
- **Doctest compilation**: 7.1s
- **Unit tests** (453 in hyalo-cli, 572 in hyalo-core): fast (<1.5s combined)

The slowest test suite at runtime is e2e_append (22 tests, 2.84s) — negligible compared to compile time.

## Options

### Option A: Single e2e test binary

Create `tests/e2e/mod.rs` with `mod` includes for each file. All 31 files become modules in one binary → one link step instead of 31.

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

- [x] Benchmark current compile time vs test execution time
- [x] Implement chosen option
- [x] Verify no tests lost in consolidation
- [x] Measure improvement

## Results

Implemented **Option A**: consolidated 31 separate e2e test binaries into a single `e2e` test binary.

- Moved `tests/e2e_*.rs` → `tests/e2e/*.rs` (stripped `e2e_` prefix)
- Moved `tests/common/` → `tests/e2e/common/`
- Created `tests/e2e/mod.rs` as the single entry point with `[[test]]` in Cargo.toml
- Updated all `use common::` → `use super::common::` in submodules

**Before:** `cargo test --workspace` took **3m13s** (31 separate link steps)
**After:** `cargo test --workspace` takes **~25s** clean build, **~1.6s** incremental
**Test count preserved:** 453 (hyalo-cli unit) + 796 (e2e) + 572 (hyalo-core) = 1821 tests

Trade-off: individual test files can no longer be run with `cargo test --test e2e_find`, but tests can still be filtered with `cargo test --test e2e find::`.
