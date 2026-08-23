---
title: "Iteration 224 — test-quality hardening (concurrency, fuzz, typed e2e, Windows, scale gate)"
type: iteration
date: 2026-08-23
status: planned
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

- [ ] T-1: add a concurrency/crash test for the write path — N processes
      running `set`/`append` on the same file asserting the file is always
      one of the valid old/new contents (never truncated/half-written); and,
      where the platform allows, a kill-mid-write test asserting no partial
      file survives (temp-file discipline). ~50-line harness per the review
- [ ] T-2: add `cargo-fuzz` targets for the four parser surfaces
      (frontmatter splice, scanner, link parser, MessagePack/snapshot
      loader), with a seed corpus from existing fixtures. Wire them into the
      manual security-review workflow (Miri + cargo-fuzz), NOT CI. Document
      how to run them
- [ ] T-3: introduce typed deserialization for e2e output assertions —
      deserialize CLI JSON into the existing typed output structs (the
      `Ext*Output` family) in a shared test helper, so shape regressions are
      compile errors. Convert the highest-value suites first (links, lint,
      find); leave file-content assertions as-is
- [ ] T-4: add Windows-gated tests for M-2 (drive-relative + ADS rejection,
      shared with [[iterations/iteration-222-security-robustness-batch]]) and
      a real-NTFS case-probe behavior test; add lexical unit tests that run
      everywhere by constructing path components directly
- [ ] T-5: convert CLI-*output* substring assertions in the worst suites to
      typed-JSON parses (rides on T-3); leave content assertions. Quantify
      before/after `.contains(` counts in the iteration outcome
- [ ] T-6: add a criterion (or scripted) scale gate on a generated
      ~14k-file synthetic vault asserting a budget on `find`/`links`; decide
      cadence (nightly CI vs. on-demand xtask gate) and record it. Must name
      what it does NOT cover so it isn't read as full perf coverage

## Acceptance criteria

- [ ] A concurrent-writer test exists and asserts the target file is never
      observed in a partial state
- [ ] `cargo-fuzz` targets exist for all four parser surfaces and run from a
      documented command (not gated in CI)
- [ ] At least the links/lint/find e2e suites assert via typed
      deserialization; a deliberate field rename breaks them at compile time
- [ ] Windows drive-relative/ADS rejection is covered by `#[cfg(windows)]`
      tests
- [ ] A scale regression gate runs against a large synthetic vault with a
      documented budget and cadence

## Non-goals

- Rewriting the entire e2e suite to typed assertions in one pass (convert the
  high-value suites; the rest is follow-on)
- Turning fuzzing into a CI gate (project preference: manual)
- Fixing the perf debt itself ([[iterations/iteration-206-links-perf-profiling]]);
  this only pins a regression budget
