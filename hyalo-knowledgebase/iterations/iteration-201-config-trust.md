---
title: Iteration 201 — config trust (no silent config discard)
type: iteration
date: 2026-08-18
status: completed
branch: iter-201/config-trust
tags:
  - iteration
  - config
  - cli
related:
  - "[[dogfood-results/dogfood-v0210-pre-release-iters-191-198]]"
---

# Iteration 201 — config trust (no silent config discard)

## Goal

Close the "config silently discarded" family from the dogfood report: an
explicit `--dir` drops the whole `.hyalo.toml` while claiming redundancy
(H-4), a single malformed key falls back to ALL defaults including `dir`
(M-2), and the hint stream contains an unmarked mutating command (M-7).
Common thread: the user's configuration stops applying without any signal
strong enough to notice, and in two cases CI goes vacuously green.

**Do NOT release; release is a separate user-gated step.**

## Context

Repros in [[dogfood-results/dogfood-v0210-pre-release-iters-191-198]]
(H-4, M-2, M-7). Anchors at `931a226`:

- H-4: config resolution in `crates/hyalo-cli/src/config.rs`
  (`ResolvedDefaults`); the `--dir` path currently reads the config only
  to print the "redundant" note, then discards it. Measured blast radius:
  `lint --dir <same-dir> --strict` = 366 files/no issues vs 50 files/
  4 warnings without the flag; `types list --dir …` = No results; views,
  `[lint]` ignores, severity overrides, site_prefix, changelog path all
  lost. `hyalo config` itself emits `--dir`-bearing hints.
- M-2: malformed `.hyalo.toml` (unknown key OR type error anywhere,
  including `[links.auto]`) → warning on stderr, then ALL defaults. `-q`
  suppresses the warning; `links auto --apply -q` can then rewrite a
  different tree than `dir` configured. The parse-twice artifact ("1
  additional identical warning(s) suppressed" every run, dogfood L-14)
  lives in the same code path — fix it while there.
- M-7: `hints.rs:1067` builds a `views set` command that WRITES
  `.hyalo.toml`, emitted in the same `-> hyalo …` list as read-only
  drill-downs when 2+ filters combine.

## Tasks [7/7]

- [x] H-4 decision first, recorded as a DEC entry: when `--dir` equals the
      configured dir, the config MUST apply (the note stays, minus the
      lie). When `--dir` names a different directory, pick one semantic
      and implement it everywhere: load that directory's own
      `.hyalo.toml` if present, else defaults — and say which config file
      is in effect on stderr. `hyalo config --dir X` must report the truth
      (`config_path` never silently null while a config was read).
- [x] H-4: stop emitting `--dir` in hints when the flag would change which
      config applies (hint builders already thread flags — reuse that).
- [x] M-2: a malformed `.hyalo.toml` must NOT fall back to all-defaults
      for mutating commands — hard error, exit 1, with today's excellent
      parse diagnostics. Read-only commands may keep the
      warn-and-defaults behavior, but the warning must survive `-q`
      (config-integrity warnings are not chatter).
- [x] M-2: fix the double-parse (config parsed twice per invocation per
      the dedup notice).
- [x] M-7: mutating hints get a distinct marker (e.g. `=> … # writes
      .hyalo.toml`) or move to a separate labelled block. The iter-192
      hint-execution e2e gate must then assert that every UNMARKED hint is
      side-effect-free (run it, snapshot-diff the vault + config).
- [x] e2e: `--dir` same-dir keeps schema/views/lint config (assert the 4
      HYALO002 warnings appear WITH the flag); malformed-config mutation
      refusal; `-q` still shows the config warning on reads.
- [x] Docs: configuration.md section on config resolution order and the
      `--dir` semantics; CHANGELOG (breaking if H-4 changes `--dir`
      behavior — call it out).

## Acceptance criteria [5/5]

- [x] `hyalo lint --dir hyalo-knowledgebase --strict` reports the same
      findings as without the flag (repo KB: 4 warnings, not vacuous
      green)
- [x] Every hint emitted by `hyalo config` runs and returns non-degraded
      results
- [x] `hyalo set`/`links auto`/any writer exits 1 on a malformed
      `.hyalo.toml` and touches nothing
- [x] No unmarked hint mutates anything (gate-enforced)
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Boundary checks for madr/changelog/new —
  [[iteration-202-boundary-completion]].
- Any change to what `[links.auto]` keys mean (iter-195a semantics stay).
