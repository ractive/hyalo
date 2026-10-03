---
title: "Iteration 306: review follow-ups — correctness"
type: iteration
date: 2026-10-04
tags: [iteration, review, correctness, cli]
status: completed
branch: iter-306/review-correctness-fixes
---

# Review follow-ups — correctness

## Problem

[[reviews/codebase-review-2026-10-03]] lists two reproducible CLI bugs (F2, F3),
a broken named-path promise (F5), envelope bypasses, a lost durability batch in
`lint --fix`, a CRLF rewrite in `init`, advisories that ignore `-q`, and a set
of hints and help texts that say something the binary does not do. This
iteration fixes the P1 Rust items, the P2 Rust items and the P3 hint items.

## Tasks

- [x] F2: batch `mv --glob` no longer panics on a 4-byte emoji filename
- [x] F3: `[[sub/rel-1.2]]` resolves to `sub/rel-1.2.md`, and `find --broken-links`, HYALO006, `summary.links.broken`, `links fix` and `backlinks` agree
- [x] F5: a gitignored named file is returned under every `--fields`; gitignore-excluded files are counted; gitignore honouring documented (DEC-342)
- [x] `apply.rs` staging-budget and `drop_index.rs` delete failure go through the JSON envelope, exit 1
- [x] `lint --fix` and `types set --default` use PerDirectory durability batching for more than 8 files (DEC-317)
- [x] `hyalo init` preserves CRLF line terminators in a host `CLAUDE.md`
- [x] Delete `MutationJournal::rename_entry` (DEC-343)
- [x] DEC-270 list-collapse note and lint/find skip notes are silenced by `-q`
- [x] `is_pid_alive` on Windows: real probe or documented fallback
- [x] `extract_fence_language` cannot panic on a mismatched precondition
- [x] An `enum` constraint without `values` is refused at config load
- [x] `views` surfaces a malformed `.hyalo.toml` diagnostic instead of "unknown view"
- [x] Hints and texts: zero-result hint, `mv --on-conflict skip` text and hint, `new --dry-run` hint, `links auto` hint, `summary --index` text layout, batch `mv` no-op reported, `lint --strict` wording, `okf.rs` module doc
- [x] `concurrent_set_never_observed_partial` no longer flakes on Windows (bounded retry of the race setup until a writer wins)

## Acceptance criteria

- [x] Every fix above has a regression test named by behaviour
- [x] Exit codes stay 0/1/2 (DEC-307)
- [x] CHANGELOG `[Unreleased]` lists each fix under "Fixed"
- [x] Help texts, `.claude/CLAUDE.md` claims and templates agree with the binary
- [x] fmt, clippy `-D warnings`, `cargo test --workspace`, `just gates`, `cargo deny check` and `hyalo lint --strict` are green
- [x] CI green on Linux, macOS and Windows
