---
title: "Iteration 224 — test-quality hardening (concurrency, fuzz, typed e2e, Windows, scale gate)"
type: iteration
date: 2026-08-23
status: completed
branch: iter-224/test-quality-hardening
tags: [iteration, testing, ci, robustness]
related:
  - "[[reviews/deep-analysis-2-2026-08-23]]"
  - "[[reviews/adversarial-review-2026-08-23]]"
---

# Iteration 224 — test-quality hardening

## Goal

Close the six test-quality gaps from
[[reviews/deep-analysis-2-2026-08-23]] (T-1..T-6). The suite is large
(~3,650 tests) and behavioral, and `hint_execution.rs` is a genuinely strong
meta-test; these are the holes it doesn't cover — the safety-critical write
path and the parser surfaces most of all.

## Context

- **T-1** — the atomic-write machinery (`fs_util.rs:204-278`: temp + fsync +
  rename + dir-fsync) is the most safety-critical code and has zero tests
  under contention or interruption: no two-process mutation race, no
  kill-mid-write, no kill between `persist` and parent-dir sync.
- **T-2** — the SEC-1/2/3 hardening, line caps, and anchor budgets were
  driven by one-off PoCs; there are no `cargo-fuzz` targets for the four
  parser surfaces (frontmatter splice, scanner, link parser, MessagePack
  loader). Per project preference: set up targets, not CI.
- **T-3** — e2e assertions index `serde_json::Value` by string
  (`json["results"]["links"]["total"]`), so output-shape regressions surface
  as `expect` panics, and nothing ties the tests to the typed output structs
  (DEC-025). Deserializing into the real structs would make a schema change a
  compile error across the e2e suite instead of a runtime discovery.
- **T-4** — Windows-only behaviors (report #1 M-2: drive-relative paths,
  ADS; case-index probe on NTFS) are untested anywhere; the CI builds
  Windows but the case-insensitivity logic runs only under `#[cfg]`-gated
  tests on whichever platforms execute them.
- **T-5** — substring assertions still dominate some suites (`links.rs`: 104
  `.contains(` vs 5 typed parses); `hint_execution.rs`'s own header explains
  why substring assertions on CLI *output* lie (they pass while the command
  fails to run). Where they assert on file *content* they're fine.
- **T-6** — no scale regression gate: `bench-e2e.sh` is manual; the known
  perf debt (iter-206 fuzzy candidates) has nothing preventing
  reintroduction.

## Tasks

- [x] T-1: add a concurrency/crash test for the write path — N processes
      running `set`/`append` on the same file asserting the file is always
      one of the valid old/new contents (never truncated/half-written); and,
      where the platform allows, a kill-mid-write test asserting no partial
      file survives (temp-file discipline). ~50-line harness per the review
- [x] T-2: add `cargo-fuzz` targets for the four parser surfaces
      (frontmatter splice, scanner, link parser, MessagePack/snapshot
      loader), with a seed corpus from existing fixtures. Wire them into the
      manual security-review workflow (Miri + cargo-fuzz), NOT CI. Document
      how to run them
- [x] T-3: introduce typed deserialization for e2e output assertions —
      deserialize CLI JSON into the existing typed output structs (the
      `Ext*Output` family) in a shared test helper, so shape regressions are
      compile errors. Convert the highest-value suites first (links, lint,
      find); leave file-content assertions as-is
- [x] T-4: add Windows-gated tests for M-2 (drive-relative + ADS rejection,
      shared with [[iterations/iteration-222-security-robustness-batch]]) and
      a real-NTFS case-probe behavior test; add lexical unit tests that run
      everywhere by constructing path components directly
- [x] T-5: convert CLI-*output* substring assertions in the worst suites to
      typed-JSON parses (rides on T-3); leave content assertions. Quantify
      before/after `.contains(` counts in the iteration outcome
- [x] T-6: add a criterion (or scripted) scale gate on a generated
      ~14k-file synthetic vault asserting a budget on `find`/`links`; decide
      cadence (nightly CI vs. on-demand xtask gate) and record it. Must name
      what it does NOT cover so it isn't read as full perf coverage

## Acceptance criteria

- [x] A concurrent-writer test exists and asserts the target file is never
      observed in a partial state
- [x] `cargo-fuzz` targets exist for all four parser surfaces and run from a
      documented command (not gated in CI)
- [x] At least the links/lint/find e2e suites assert via typed
      deserialization; a deliberate field rename breaks them at compile time
- [x] Windows drive-relative/ADS rejection is covered by `#[cfg(windows)]`
      tests
- [x] A scale regression gate runs against a large synthetic vault with a
      documented budget and cadence

## Non-goals

- Rewriting the entire e2e suite to typed assertions in one pass (convert the
  high-value suites; the rest is follow-on)
- Turning fuzzing into a CI gate (project preference: manual)
- Fixing the perf debt itself ([[iterations/iteration-206-links-perf-profiling]]);
  this only pins a regression budget

## Outcome

- **T-1**: `crates/hyalo-cli/tests/e2e/concurrent_writes.rs` — two tests, both
  designed so their assertions hold regardless of exact scheduling (no
  sleep-and-hope, nothing `#[ignore]`d). `concurrent_set_never_observed_partial`
  races 12 `hyalo set` processes on one file with a reader thread continuously
  re-reading it; along the way this confirmed `set`'s `check_mtime` TOCTOU
  guard correctly refuses losing writers under contention rather than
  corrupting anything (expected, not a bug — the test tolerates some
  processes losing the race). `kill_mid_write_never_leaves_torn_destination`
  (`#[cfg(unix)]`) polls for `atomic_write`'s sibling `.tmp*` file rather than
  guessing a sleep, then `SIGKILL`s, then accepts either the original or a
  complete new write — never a partial one. 30+ repeated local runs, 0 flakes.
- **T-2**: `fuzz/` — a `cargo-fuzz` crate excluded from the parent workspace
  (own `[workspace]` table), four targets (`frontmatter_splice`, `scanner`,
  `link_parser`, `snapshot_loader`), seed corpus in `fuzz/seeds/<target>/`
  drawn from real knowledgebase files plus hand-written edge cases (empty
  file, unterminated frontmatter, unbalanced `[[`). All four ran clean
  ~15s smoke sessions (hundreds of thousands of link_parser execs, tens of
  thousands for the others) with zero crashes. `fuzz/README.md` documents
  setup, running, and what it does not cover. Not wired into CI (confirmed:
  `cargo metadata` at the repo root does not see the `fuzz` package at all).
- **T-3/T-5**: shared `typed_results::<T>()` helper added to
  `tests/e2e/common/mod.rs` (deserializes the standard `{results, hints,
  total}` envelope). `Deserialize` added alongside existing `Serialize` on
  the real production output structs: `hyalo_core::types::{FileObject,
  LinkInfo, BacklinkInfo, PropertyInfo, ContentMatch, VaultSummary,
  LinkHealthSummary, FileCounts, TagSummary, PropertySummaryEntry, ...}` and
  `commands::lint::{ExtLintOutput, ExtLintFixOutput, ExtFileLintResult,
  RuleGroup, BodyViolation, ...}` — the structs that actually back live CLI
  JSON (an earlier pass wrongly targeted `lint.rs`'s `LintOutput` family,
  which turned out to be dead code only reachable from that file's own unit
  tests; corrected before converting any lint.rs assertions). `links.rs`
  additionally got a test-local `LinksFixResults` struct mirroring `links
  fix`'s dynamically-built `json!()` output, since that command has no
  production struct to reuse yet (its per-fix-plan arrays are built via
  post-processing that attaches a computed `rule` field — turning that into
  a real struct is follow-on work, noted in the struct's own doc comment).
  Compile-error property verified for all three suites: temporarily renaming
  a converted field (`LinksFixResults::broken`, `FileObject.file`,
  `ExtLintOutput.errors`) breaks `cargo check --test e2e` at the exact
  converted call sites every time; reverted after each check.

  Quantified before/after (`.contains(` stayed flat in all three — it turns
  out almost none of those were JSON-`Value` indexing in the first place, so
  it undercounts the actual conversion; `["results"]` bracket-indexing is the
  more accurate signal for CLI-JSON-output access and dropped substantially):

  | Suite | `.contains(` before → after | `["results"]` before → after | tests converted |
  |---|---|---|---|
  | links.rs | 111 → 111 | 85 → 75 | 43 (38 `links fix` helper call sites + 5 `summary`/`backlinks`) |
  | find.rs | 89 → 89 | 18 → 2 | ~154 of 178 test functions (two shared helpers converted + individual sites) |
  | lint.rs | 101 → 101 | 56 → 5 | ~30 |
  | **total** | 301 → 301 | 159 → 82 | ~227 |

  Remaining `["results"]` sites are documented as deliberately untyped: they
  assert a key's *absence* from JSON (not just null/default, which a typed
  struct can't distinguish), or read commands out of scope for this pass
  (`summary`/`lint-rules` sites inside `lint.rs`, `links auto`'s dynamic
  per-title output).
- **T-4**: new `crates/hyalo-cli/tests/e2e/windows_paths.rs`
  (`#![cfg(windows)]`) — 5 CLI-level tests covering both M-2 gates
  (drive-relative `C:foo.md`, NTFS-ADS `a.md:stream`) through both `read`
  and `set --file`, plus a false-positive check on an ordinary nested path.
  `case_index.rs` gained two `#[cfg(windows)]` tests pinning that NTFS is
  case-insensitive by default (the existing tests deliberately don't assert
  a direction since they run on every platform). `discovery.rs` gained a
  cross-platform lexical shape table (`has_unsafe_windows_colon_shape_table`)
  covering more colon shapes than the single pre-existing case. Cannot run
  any of this locally (macOS host, and cross-compiling to
  `x86_64-pc-windows-msvc` fails on missing MSVC headers for two transitive
  C deps — `onig_sys`, `alloca` — no Windows SDK available here); written by
  close pattern-matching against the existing passing `#[cfg(windows)]`
  tests and verified to parse/typecheck on this host wherever cfg allowed.
- **T-6**: `cargo run -p xtask -- bench-scale`
  (`crates/xtask/src/bench_scale.rs`) — deterministic ~14k-file synthetic
  vault (pure function of file index, no RNG seed to manage), times `find`
  and `links fix` (median of 3 runs each) against a fixed budget. Measured
  local baseline (Apple Silicon): `find` ~0.44s (budget 3s, ~7x headroom),
  `links fix` ~3.45s (budget 15s, ~4x headroom — lower multiple since
  `links fix` does real cross-file resolution work that scales with corpus
  size). On-demand only, not CI — cadence and rationale in DEC-098.
- **Gates**: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D
  warnings`, and `cargo test --workspace -q` all clean, run twice with no
  flakes (1676 e2e + 1195/79/28 unit tests across the three crates plus
  doctests, all green both runs). All four `xtask check-*` gates and
  `bench-scale` itself pass unaffected by the new `bench-scale` subcommand.
- **No real bugs found** in production code by any of the new tests — this
  iteration was test-infrastructure-only as scoped, aside from the two
  structural fixes to `commands/lint.rs` (moving `Deserialize` from the dead
  `LintOutput` family onto the live `ExtLintOutput` family, discovered via
  this same conversion work) and the `#[serde(default)]` additions needed
  for a handful of `skip_serializing_if`-on-non-`Option` fields to round-trip
  through `Deserialize` cleanly.
