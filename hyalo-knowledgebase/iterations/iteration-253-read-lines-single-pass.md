---
type: iteration
title: "Iteration 253 — read: compute lines without a second full-file scan"
date: 2026-08-30
status: planned
tags:
  - iteration
  - agent-cli
  - performance
depends-on: "[[iterations/iteration-252-find-result-shape]]"
branch: iter-253/read-lines-single-pass
---

# Iteration 253 — `read`: compute `lines` without a second full-file scan

## Goal

Carry-over from [[iterations/iteration-252-find-result-shape]] review (PR
#293, not a blocker — merged as-is). `hyalo read` now always reports `lines`
(iter-252), computed via `scanner::count_file_lines(&full_path)` —
an unconditional second full read of the file from disk.

Whenever the read needs the body anyway (`need_body == true`: no
`--frontmatter`-only request, or `--section`/`--lines` given), the file's
body is *already* fully read into memory via `read_body_lines` before that
second scan runs. Since `read_body_lines` reads every body line and
`frontmatter::skip_frontmatter` already returns the frontmatter's line
count, the total line count matching `scanner::count_lines`'s definition can
be derived from data already in hand — no second disk pass.

The `--frontmatter`-only path (`need_body == false`) does need a dedicated
scan to get `lines`, since the body was deliberately not read there — that
call to `count_file_lines` stays.

This is a performance-only finding (no incorrect output), so it is not a
blocker on #293; low absolute cost per single-file CLI invocation, but worth
fixing since it doubles disk I/O for exactly the large-file case this
feature exists to help with, and contradicts the "frontmatter-only queries
pay zero cost for body scanning" principle ([[decision-log]], scanner
section) for the one case still forced to eat a full second read anyway.

## Tasks

- [ ] `read_body_lines` returns the frontmatter line count alongside the body
      lines (e.g. `(Vec<String>, usize)`), without adding I/O — it already
      has this number from `skip_frontmatter`, it just isn't propagated.
- [ ] In `commands/read.rs::run`, capture the raw body line count right after
      `read_body_lines` returns (before `--section`/`--lines` narrow
      `content_lines`), and compute `total_lines = fm_lines + raw_body_lines`
      when `need_body` was true. Keep `scanner::count_file_lines` only for
      the `need_body == false` (`--frontmatter`-only) path.
- [ ] Unit/e2e test: `total_lines` computed this way matches
      `scanner::count_lines` applied to the whole file, across the same
      CRLF / no-trailing-newline / invalid-UTF-8 / oversized-line cases the
      iter-252 suite already covers for `find` — reuse or extend
      `find_result_shape.rs`'s baseline-comparison helper rather than
      duplicating it.
- [ ] Confirm no behavior change for `--frontmatter`-only reads (still one
      full-file scan, not zero — `lines` requires reading the file; this
      iteration only removes the *second* read on the already-body-reading
      paths).

## Acceptance criteria

- [ ] `read` without `--frontmatter`-only reads the file's bytes once, not
      twice (verified by the new test comparing against the pre-253 double-
      read behavior, or by inspecting call sites — no new I/O-counting
      instrumentation needs to ship).
- [ ] `read`'s reported `lines` is byte-identical to today's for every
      existing `find_result_shape.rs` / `read` e2e case (CRLF, UTF-8,
      no-EOL, frontmatter-only, `--section`, `--lines`).
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`.

## Non-goals

- Changing what `lines` means, or which `read` invocations report it —
  iter-252 already settled that (every `read` result carries `size`/`lines`
  matching the whole file). This iteration only changes how the number is
  computed.
- The `--frontmatter`-only path's forced full scan is out of scope: reporting
  `lines` there requires reading the file no matter what, by the iter-252
  contract itself.

## Links

- [[iterations/iteration-252-find-result-shape]]
- [[decision-log#DEC-252]]
