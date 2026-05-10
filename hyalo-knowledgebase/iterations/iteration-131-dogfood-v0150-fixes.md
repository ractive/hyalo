---
title: >-
  Iteration 131 — Dogfood v0.15.0 fixes (find --file abs-path, lint-rules set
  no-op, summary dir, banner polish)
type: iteration
date: 2026-05-10
status: completed
branch: iter-131/dogfood-v0150-fixes
tags:
  - iteration
  - bug-fix
  - ux
  - lint
  - find
related:
  - "[[dogfood-results/dogfood-v0150-iter127-130]]"
  - "[[iterations/iteration-128-llm-misuse-warning]]"
  - "[[iterations/iteration-130-cwd-aware-help-and-config]]"
---

## Goal

Address all findings from [[dogfood-results/dogfood-v0150-iter127-130]] against
v0.15.0: one HIGH `find --file` correctness bug, one MEDIUM config-write no-op
bug, one LOW JSON-envelope verification, and three LOW UX polish items. None
of these affect data integrity, but BUG-1 silently breaks the standard
hyalo skill hint chain whenever an LLM passes the absolute path it just saw
in another tool's output — which iter-128 was specifically designed to
forgive.

Numbering mirrors the dogfood report for traceability.

## Context

- iter-128 added absolute-path canonicalisation for `--file` consumers and CWD-in-vault
  detection. `find --file` was missed: it emits the warning but does not strip
  the prefix before matching, so the result set is silently empty.
- iter-127 fixed `lint-rules set --severity` panicking on scalar→table promotion. The
  follow-up problem found in v0.15.0 is the inverse: setting a property to its
  current default value materialises a redundant `[lint.rules.X]` table while
  reporting "(no change)".
- iter-130 added a CWD-aware help banner and `hyalo config`. The banner uses
  emojis unconditionally and the redundant-`--dir` notice is double-prefixed
  `warning: note:`.

## High

### BUG-1: `find --file <abs-path-inside-vault>` returns 0 results

**Bug:** `hyalo find --file /Users/james/devel/hyalo/hyalo-knowledgebase/iterations/iteration-130-cwd-aware-help-and-config.md`
emits the iter-128 abs-path warning, but matches against the unstripped
absolute path and returns `total: 0`. Other `--file` consumers (`set --file`,
`set <pos>`, `backlinks`, positional file args) handle this correctly. The
relative form (`--file iterations/...`) works.

**Fix:** Route `find --file` through the same path-canonicalisation helper used
by `set`/`backlinks`. Apply the iter-128 abs-path-inside-vault stripping
before comparing against the indexed file paths.

- [x] Locate the canonicalisation helper introduced in iter-128 (likely in
      `hyalo-cli` path utilities) and confirm what `set --file` calls
- [x] Trace `find --file` argument handling and identify where the path is
      compared (raw string vs canonicalised)
- [x] Route `find --file` through the same canonicalisation call so the
      warning and the match agree
- [x] E2E test: `find --file <abs-path-inside-vault>` matches exactly one file
      and emits the warning to stderr
- [x] E2E test: `find --file <relative-path>` continues to work unchanged
- [x] E2E test: `find --file <abs-path-outside-vault>` errors clearly (no
      silent empty result) — see UX-2 below
- [x] Cross-check every other `--file`-accepting subcommand (`backlinks`,
      `read`, `links`, etc.) for the same regression and add a single
      shared unit test if helpful

## Medium

### BUG-2: `lint-rules set --severity <default>` writes redundant TOML sections

**Bug:**

```text
$ hyalo lint-rules set HYALO002 --severity error
HYALO002: (no change)
  wrote /Users/james/devel/hyalo/.hyalo.toml
```

The default severity for HYALO002 is already `error`. The CLI reports
"(no change)" but the file diff shows a brand-new section appended:

```toml
[lint]

[lint.rules]

[lint.rules.HYALO002]
severity = "error"
```

Two coupled issues:
1. "(no change)" message contradicts the file write.
2. Setting a property to its current default should be a true no-op — don't
   materialise the override or its parent sections.

**Fix:** In `lint-rules set`, before writing, compare the requested value
against the rule's *default* (not just the previous override). If the
result of the set would be a tautological override, skip the write
entirely. If the override section becomes empty after the set, prune
`[lint.rules.X]`; if `[lint.rules]` becomes empty, prune it; same for
`[lint]`.

- [x] Identify where `lint-rules set` decides whether to write
- [x] Add a "would-be-default" check: compare new effective value to the
      built-in default, skip materialisation when equal and no other
      keys exist for the rule
- [x] Prune empty parent tables after removing the last child key (so
      consecutive `set` / unset cycles don't leave orphan headers)
- [x] E2E test: setting severity to the default with no prior override is a
      no-op — file unchanged, exit 0, output says "(no change)" without
      "wrote ..."
- [x] E2E test: setting severity to the default *when an override existed*
      removes the override and prunes empty parents
- [x] E2E test: setting a non-default value materialises the override
      exactly as before (regression guard for iter-127 promotion fix)

## Low

### BUG-3: `hyalo summary --format json` may omit top-level `dir` field

**Bug:** iter-130 AC required the JSON `summary` envelope to expose the
resolved `dir`. Verification was inconclusive — the text output renders
`kb dir:` correctly, but a top-level `dir` JSON field could not be located
in piped output. May be nested or missing.

**Fix:** Verify the current shape; if `dir` is missing or buried, surface it
at the top level alongside `total`, etc.

- [x] Inspect `summary --format json` output on own KB, MDN, and GitHub Docs
- [x] If `dir` is absent or nested, add a top-level `dir: <resolved-path>` field
- [x] Update the JSON-envelope docs (README + iter-130 reference if needed)
- [x] E2E test: `summary --format json` includes a top-level `dir` matching
      the effective dir from `hyalo config --format json`

### UX-1: Redundant `--dir` warning is double-prefixed `warning: note:`

**Bug:** `warning: note: --dir is redundant; .hyalo.toml already sets dir = "hyalo-knowledgebase"`.
Both prefixes are present; conventional CLI style picks one.

**Fix:** Drop one prefix. Prefer `note: ...` to match the iter-130 plan
language ("one-line stderr note") and keep `warning:` reserved for things
that might break a workflow.

- [x] Locate the emit site for the redundant-`--dir` notice
- [x] Change to a single `note:` (or `warning:`) prefix consistently
- [x] E2E test asserts the exact prefix appears once

### UX-2: `find --file <abs-path>` failure mode is silently empty

**Bug:** Coupled with BUG-1. Even after BUG-1 is fixed, an absolute path
*outside* the vault should not silently return `total: 0`. The warning
already detected the mismatch — escalate to a clear error so the user
retries with a relative path.

**Fix:** When `--file` receives an abs path that is not inside the vault,
return a non-zero exit with `error: --file <path> is outside the vault
dir <kb-dir>; pass a relative path or move the file in.`

- [x] Detect "abs path outside vault" in the canonicalisation helper
- [x] Surface as an error, not a warning + empty result
- [x] E2E test: abs path outside vault exits non-zero with helpful message

### UX-3: Banner emojis ride piped output

**Bug:** The CWD-aware help banner emits `ℹ️ ` / `⚠️` regardless of TTY. When
help is piped to a file or another tool, the emojis end up in the output.

**Fix:** Suppress the emojis (or the whole banner) when stdout/stderr is not
a TTY. The same TTY detection iter-129 already wired up for `--format` should
cover this.

- [x] Identify the banner emit site (added in iter-130)
- [x] Gate emoji rendering on `IsTerminal` for the destination stream
- [x] Keep the banner text itself when piped, just drop the emoji prefix —
      or drop the whole banner; pick whichever is more useful for agents
      and document the choice
- [x] E2E test: `hyalo --help 2>&1 | cat` produces banner without the emoji

## Quality gates (per CLAUDE.md, in order, must pass)

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace -q`

## Acceptance criteria

- [x] BUG-1: `hyalo find --file <abs-path-inside-vault>` returns the matching
      file (not empty) and emits the iter-128 warning
- [x] BUG-2: `hyalo lint-rules set X --severity <default>` is a true no-op
      when no prior override exists; prunes empty parent tables when removing
      the last override
- [x] BUG-3: `hyalo summary --format json` exposes a top-level `dir` field
      matching `hyalo config`'s effective dir
- [x] UX-1: redundant-`--dir` notice uses a single prefix, not
      `warning: note:`
- [x] UX-2: `find --file <abs-path-outside-vault>` exits non-zero with a
      clear error rather than `total: 0`
- [x] UX-3: help banner emojis are suppressed in piped output
- [x] All quality gates pass
- [x] Dogfood follow-up: the v0.15.0 dogfood report is updated to mark these
      bugs FIXED, or a new dogfood report supersedes it

## Out of scope

- Re-architecting the warning/error/note severity vocabulary across the CLI
  (UX-1 is a one-site fix, not a sweep)
- Any changes to `hyalo config` beyond what BUG-3 may require
- New `--file` capabilities beyond bringing `find --file` in line with the
  iter-128 contract
