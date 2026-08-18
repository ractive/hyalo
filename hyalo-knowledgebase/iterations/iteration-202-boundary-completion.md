---
title: Iteration 202 — vault boundary completion and symlink walker dedup
type: iteration
date: 2026-08-18
status: planned
branch: iter-202/boundary-completion
tags:
  - iteration
  - write-path
  - security
related:
  - "[[dogfood-results/dogfood-v0210-pre-release-iters-191-198]]"
  - "[[iterations/iteration-191-write-path-integrity]]"
---

# Iteration 202 — vault boundary completion and symlink walker dedup

## Goal

Extend iter-191's boundary-checked writes to the three writers it missed
(dogfood H-3, M-3, M-4) and fix the walker's symlink double-enumeration
(M-5), which makes `links fix --apply` false-fail in CI. Also unify the
refusal messages/exit codes the dogfood flagged as inconsistent.

**Do NOT release; release is a separate user-gated step.**

## Context

Repros in [[dogfood-results/dogfood-v0210-pre-release-iters-191-198]]
(H-3, M-3, M-4, M-5, L-16). Anchors at `931a226`:

- H-3: `crates/hyalo-cli/src/commands/madr.rs:78` builds
  `<adr-dir>/README.md` from a positional dir with no boundary validation
  — plain `../` traversal writes/modifies files outside the vault at
  exit 0 (`--apply` only; `--dry-run` is safe).
- M-3: `commands/changelog.rs` follows a `CHANGELOG.md` symlink that
  resolves outside the vault; the structurally identical `okf log`
  refuses correctly — copy its check.
- M-4: `commands/new.rs` validates lexically (`..`/absolute rejected with
  good messages) but never canonicalizes, so `outdir -> ../outside`
  inside the vault is a write-out primitive for file creation.
- M-5: the vault walker enumerates an in-vault symlink AND its target as
  two files. `links fix --apply` then rewrites the same note twice; the
  second write trips the concurrency guard → "modified by another
  process", exit 1, though the fix landed. Same root double-counts
  `find --count`/`summary`/glob-write counters and prints the
  out-of-vault-symlink skip warning twice.
- L-16: `okf log` boundary refusal exits 2 (documented "internal error"
  class) vs 1 everywhere else; `mv` uses two phrasings for one refusal
  class. The best message is `okf log`'s two-path form (in-vault path +
  resolved target) — iter-191's `set` family only prints the typed path.

## Tasks

- [ ] H-3: `madr toc` canonicalizes the ADR dir (following symlinks) and
      refuses when the resolved `README.md` falls outside the vault —
      same `atomic_write_within` path the iter-191 writers use. e2e for
      the `../` and symlinked-dir vectors.
- [ ] M-3: `changelog add`/`release --apply` resolve `CHANGELOG.md`
      through symlinks and refuse an out-of-vault target, mirroring
      `okf log`. Note: an INTENTIONAL out-of-vault changelog is
      configured via `[changelog] path` — that documented path stays
      allowed; only the silent symlink escape is refused.
- [ ] M-4: `new --file` canonicalizes the parent directory after the
      lexical checks and refuses out-of-vault resolution with the same
      message `set` gives.
- [ ] M-5: dedup vault enumeration by canonical path (first spelling
      wins, stable order). Asserts: `links fix --apply` with an in-vault
      symlink exits 0 and rewrites once; `find --count` counts one file;
      the skip warning prints once. Watch Windows: canonicalize via the
      existing helpers, no `\\?\` surprises (CI covers it).
- [ ] Unify refusal UX: one exit code (1) and one message shape for the
      whole boundary family, adopting the two-path phrasing ("path X
      resolves outside vault Y"). Update the e2e escape-refusal suite.
- [ ] Sweep for other unchecked writers: grep every `--apply`/write
      command for missing canonicalize+boundary (okf index, lint --fix,
      task, set/append/remove already covered — verify and list in the
      PR body).

## Acceptance criteria

- [ ] All H-3/M-3/M-4 repros from the report refuse at exit 1 with
      nothing written outside the vault
- [ ] `links fix --apply` on the M-5 repro vault exits 0; the note is
      rewritten exactly once
- [ ] `summary` on a vault with an in-vault symlink to one note reports
      1 file
- [ ] Boundary refusals share exit code and message shape (asserted in
      e2e)
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Read-side symlink policy changes beyond dedup (a symlink pointing
  outside stays a skip-with-warning).
- L-3 (chmod 444 / hardlink semantics of atomic writes) and L-4 (mv onto
  dangling symlink) — batch iteration
  [[iteration-204-dogfood-low-batch]].
