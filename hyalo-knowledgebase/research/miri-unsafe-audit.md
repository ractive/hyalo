---
title: Miri Evaluation & Unsafe Audit
type: research
date: 2026-05-23
tags:
  - miri
  - unsafe
  - safety
  - testing
  - performance
status: active
---

# Miri Evaluation & Unsafe Audit

How Miri fits into hyalo's test surface, what it found, and how the remaining
`unsafe` should be approached. Companion to [[decision-log]].

## Setup

Miri ships only on nightly:

```bash
rustup toolchain install nightly --component miri
just miri              # runs the parsing surface of hyalo-core
just miri-filter <pat> # targeted module run
just miri-all          # everything (most fs tests skip)
```

The `justfile` recipes pass `MIRIFLAGS="-Zmiri-disable-isolation"` so tempfile
+ filesystem-touching tests can run, and `--test-threads=1` because Miri is
interpreted and parallel test output is unreadable.

## What can and can't run under Miri

| Category | Miri-safe | Notes |
|---|---|---|
| Pure parsers (scanner, frontmatter, bm25, links, heading, filter) | ✅ | These are the high-value targets — they exercised the `unsafe` UTF-8 calls. |
| `tempfile` / process spawn / mmap | ❌ | `tempfile` calls `libc::fchmod` internally; Miri can't shim it on macOS. All e2e tests spawn the binary and are excluded. |
| `rayon` parallel iterators | ❌ | `par_iter` doesn't work under Miri. Worked around with `#[cfg(not(miri))]` + serial fallback in `index.rs` and `lint.rs`. |
| `regex` / `aho-corasick` | ⚠ runs but very slow | State-machine construction is pathological under interpretation; one batched run got killed after 25 h still building tests. Use module-by-module runs with reasonable expectations. |

## Audit results — 2026-05-23

Before this audit, hyalo had four `unsafe` blocks:

1. `scanner/strip.rs:74` — `String::from_utf8_unchecked` after ASCII backtick→space substitution
2. `scanner/strip.rs:156` — `String::from_utf8_unchecked` after ASCII `%%`→space substitution
3. `scanner/mod.rs:105` — `std::str::from_utf8_unchecked` after a separate `is_ok()` validation check
4. `index.rs:825` — `libc::kill(pid, 0)` for PID liveness

PR #158 removed (1)–(3):

- (1) and (2) now go through `String::from_utf8(...).expect(...)`. Microbench
  cost: +5 ns per call when backticks/comments are present; +0 ns on the fast
  path (no backticks). MDN 250 MB end-to-end: no measurable regression.
- (3) was a redundant validation. Replaced with a match that reuses the
  existing `Result::Ok(s)` from the upfront UTF-8 check — zero perf cost, no
  re-validation, no unsafe.

`libc::kill(pid, 0)` remains. No portable std equivalent for "is this PID
alive?". Documented SAFETY block; not exercised by any test path (so Miri
can't validate it), but it's a one-line FFI call with well-defined invariants.

## Miri runs — outcomes

262 tests across 5 modules passed under Miri with no UB detected:

| Module | Tests | Result | Wall time |
|---|---|---|---|
| `scanner::` | 60 passed, 1 ignored | ✅ no UB | ~6.5 min |
| `bm25::` | 49 passed, 1 failed¹ | ⚠ pre-existing brittle test | 16 s |
| `links::` | 50 passed | ✅ no UB | 6.2 s |
| `heading::` | 38 passed | ✅ no UB | 53 s |
| `frontmatter::` | 65 passed | ✅ no UB | 29 s |
| `content_search::` | not completed | killed during run² | n/a |
| `filter::` | not run | — | n/a |

¹ `bm25::tests::test_bm25_serde_round_trip` — see "Known issues" below.

² `content_search::` and `filter::` both pull in `regex` + `aho-corasick` and
are intractably slow under Miri. They can be run individually with a wall-clock
cap if needed, but the payoff is low — these modules have no `unsafe`.

## Known issues surfaced by Miri

### bm25 round-trip uses too-tight f64 tolerance

`bm25::tests::test_bm25_serde_round_trip` at `bm25.rs:1504` asserts:

```rust
assert!((before[0].score - after[0].score).abs() < f64::EPSILON);
```

`f64::EPSILON` (~2.22e-16) is too tight for BM25 scores that are sums of
multiple floats. Miri seeds the `HashMap` hasher differently than native,
producing a different iteration order — and therefore a different summation
order — yielding differences around `1e-15`.

Not UB; just a brittle native test that happens to expose order-dependent
float summation. Fix when convenient: widen tolerance to e.g. `1e-9` or
`f64::EPSILON * before[0].score`.

## Recommendation

- Treat Miri as a manual gate, not a CI gate (consistent with the existing
  feedback note on Miri + cargo-fuzz being manual).
- Run `just miri` after refactors that touch `scanner/`, `frontmatter/`, or
  any future code with `unsafe`.
- Don't bother with `regex`-heavy modules unless a specific `unsafe` lands there.
- If a future contribution adds `unsafe`, the safety doc block should reference
  this evaluation so the next refactor has context for whether the unsafe is
  paying its way.
