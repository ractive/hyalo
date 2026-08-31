---
type: iteration
title: "Iteration 257 — init/deinit --dir scoping and JSON envelope gap"
date: 2026-08-31
status: planned
tags:
  - iteration
  - dogfood-fixes
  - bug
branch: iter-257/init-deinit-dir-scope
depends-on: "[[iterations/iteration-256-envelope-help-forwarding-and-index-cost]]"
---

# Iteration 257 — `init`/`deinit` `--dir` scoping and JSON envelope gap

## Goal

Carry-over from [[iterations/iteration-256-envelope-help-forwarding-and-index-cost]]
(PR #298), filed under "Dogfooding findings (filed, not fixed here)" rather
than fixed in that iteration because they are `init`/`deinit`-specific bugs,
not envelope/help-text coherence work. Both items below were hit live during
256's own dogfooding pass: an `init --dir <other-tree>` / `deinit` probe from
inside this repo silently clobbered this repo's own `.hyalo.toml` and deleted
`.claude/CLAUDE.md` plus three `.claude` symlinks, restored by hand in that
branch (commits `78fc09b5`, `0c96ec77`).

## Tasks

### BUG-1: `init --dir <other-tree>` writes an absolute, self-refusing `dir` [0/2]

- [ ] Reproduce: from a directory with no `.hyalo.toml`, run
      `hyalo init --dir <some-other-tree>`. Confirm it writes `.hyalo.toml`
      into the **current** directory (not `<some-other-tree>`) with
      `dir = "<absolute path to some-other-tree>"` — a value `hyalo config`
      then refuses on every subsequent run ("an absolute path, which a
      project-local `.hyalo.toml` is not allowed to set", per the
      project-local-`dir`-must-stay-at-or-below-config-directory rule added
      in iter-243/244).
- [ ] Fix `init` so it never writes a config it will immediately refuse to
      read: either write `dir` as a path relative to the config file's own
      directory (matching how a hand-written `.hyalo.toml` is expected to
      look), or write the config into `<other-tree>` instead of CWD, or
      refuse the combination outright with a clear error naming the
      constraint. Pick one and record it as a DEC — this is a behavior
      decision, not a one-line fix. Add an e2e test pinning the choice.

### BUG-2: `deinit` ignores `--dir` and always targets CWD [0/2]

- [ ] Reproduce: from this repo, run `hyalo --dir <temp-vault> deinit` and
      confirm it deletes **this repo's** `.hyalo.toml`, `.claude/CLAUDE.md`,
      and the three `.claude` symlinks into
      `crates/hyalo-cli/templates/` — not anything under `<temp-vault>` —
      with no warning, and a summary that interleaves the real removals with
      a dozen `skipped … (not found)` lines in a way that reads like a
      no-op at a glance.
- [ ] Fix: either honour `--dir` for `deinit`'s target the same way other
      commands do, or refuse to run when `--dir` names a tree other than
      CWD (erring on the side of not silently deleting the wrong tree's
      integration files). Record as a DEC alongside BUG-1's — the two share
      a root cause shape (an `init`/`deinit` pair that special-cases CWD
      while every other command follows `--dir`) and may share a fix.
      Add an e2e test that a `--dir`-scoped `deinit` cannot touch CWD's
      files (or is refused).

### BUG-3: `--format json` is ignored by `init`/`deinit` [0/1]

- [ ] `init` and `deinit` always print their text summary regardless of
      `--format json`, unlike every other command. DEC-257 (iter-256) named
      them as outside the mutation-envelope contract entirely (they write
      config, not notes), so this is not a COH-9 violation — but an agent
      piping `--format json` still gets unparseable text. Decide: give them
      a minimal JSON envelope (what did it write/delete, dry-run or not), or
      document that `--format` has no effect on these two commands and
      point agents at exit code + stderr for scripting. Record as a DEC.
      Update `rule-knowledgebase.md`/`skill-hyalo.md` if the contract text
      changes.

## Acceptance criteria

- [ ] BUG-1 is fixed or explicitly won't-fixed with a DEC; `init --dir
      <other-tree>` no longer produces a `.hyalo.toml` that refuses itself
      on the next run.
- [ ] BUG-2 is fixed or explicitly won't-fixed with a DEC; a `--dir`-scoped
      `deinit` cannot silently delete a different tree's integration files.
- [ ] BUG-3 has a recorded decision (JSON envelope added, or documented as
      out of scope for `--format`) — not left ambiguous.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, all `xtask check-*`, `hyalo lint --strict`.

## Non-goals

- Re-designing `init`/`deinit`'s overall UX beyond the `--dir`-scoping bug —
  this is a targeted fix, not a rework.

## Links

- [[iterations/iteration-256-envelope-help-forwarding-and-index-cost]]
- [[iterations/iteration-254-dogfood-v0220-help-and-shape-fixes]]
