---
type: iteration
title: "Iteration 331: a bulk write phase survives power loss"
date: 2026-11-07
status: completed
tags: [iteration, write-path, durability]
branch: iter-331/bulk-write-durability
related:
  - "[[decision-log]]"
  - "[[backlog/done/bulk-rewrite-durability-against-power-loss]]"
---

# Iteration 331: a bulk write phase survives power loss

DEC-317 and DEC-404 let a bulk write phase rename notes before their content was on
disk: after power loss a note could be empty and the original gone. The owner chose
the safer design: write every temporary file, flush once, then rename them all, then
fence the directories. After power loss every note is whole, old or new; a set of
notes may be a mix. Decision: DEC-408 (amends DEC-404, DEC-317, DEC-323). Backlog:
[[backlog/done/bulk-rewrite-durability-against-power-loss]].

## Tasks

### Design

- [x] Audit what breaks when a write is deferred: a note written twice in a phase, a note read after it was written, the lost-update guard, failure semantics, the journal, temp files, open handles, memory, disk space
- [x] Measure the flush primitives on macOS (per-file `fsync` plus one `F_FULLFSYNC` against `sync_all` per file)
- [x] `rooted/barrier.rs`: `fsync` per file and one `F_FULLFSYNC` per device on macOS, one `syncfs` per filesystem on Linux, `sync_all` per file on Windows and other Unix
- [x] `BulkRewrite` stages and flushes at write time, `finish_staged` runs barrier, renames, fences and returns a `FinishReport`
- [x] A second write to a staged note replaces the first and starts from its staged bytes (`OwnedPublication::capture_verified` on a staged entry)
- [x] The source is verified again at the rename, on the pool
- [x] The 8-note special case is gone: one path for two or more notes, `PerFile` for one (`BULK_REWRITE_MAX` removed)
- [x] `lint --fix`, `PreparedChangeSet` and `link_rewrite` settle their effects against the report: a note that was not renamed is `FailedBeforeCommit`/not applied

### Proof

- [x] Order of operations read off the file system at each step (all temp files exist and every note is old at the barrier; no rename before it; fences last)
- [x] One barrier per `st_dev`
- [x] A fault at the 4th staging step, at the barrier and at the 3rd rename: states as DEC-408 says, no temp file left
- [x] A note edited while staged is not overwritten; a second write still guards against an edit
- [x] A phase of 1 500 notes holds no descriptor per note; the CLI runs 400 notes under `ulimit -n 64`
- [x] Every behaviour test fails under its mutation, 20 of 20 runs, and passes 20 of 20 without it
- [x] Byte-identical trees, JSON, stderr and exit codes against the `cce1c8c8` build for `lint --fix`, `tags rename`, `properties rename`, `set --glob`, `mv`, `links fix --apply --apply-fuzzy` on the Obsidian Hub, GitHub Docs and MDN (16 cells); a binary that skips some renames fails the tree comparison in all 6 cells tried (its JSON is identical, so the tree is the check)
- [x] Hub and MDN `lint --fix`, `set --glob`, a phase of 8 notes and the peak temp-file count, median of 5, baseline and head interleaved

### Documentation

- [x] DEC-408
- [x] CHANGELOG, `.claude/CLAUDE.md`, the bundled rule and skill
- [x] Backlog item closed into `backlog/done/`

### Gates

- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -q`
- [x] The xtask gates, `generate-npm-packages --check`, `cargo deny check`, the npm gate
- [x] `hyalo lint --strict` and the CI lint-kb command
- [x] Case-sensitive volume e2e
