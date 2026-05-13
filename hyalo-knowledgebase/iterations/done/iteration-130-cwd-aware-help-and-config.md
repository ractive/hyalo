---
title: Iteration 130 — CWD-aware help banner and `hyalo config`
type: iteration
date: 2026-05-10
tags:
  - ux
  - llm
  - dx
  - cli
status: completed
branch: iter-130/cwd-aware-help-and-config
---

## Goal

Make hyalo self-explanatory to LLM-driven shells about how `.hyalo.toml`
resolves the vault `dir`, so agents stop `cd`-ing into the kb folder or
passing redundant `--dir` flags. Iteration 128 added a stderr warning when
CWD is inside the vault; this iteration extends the same teaching surface
to `--help`, `--version`, `hyalo summary`, redundant `--dir`, and adds a
new `hyalo config` subcommand that prints the effective configuration.

## Approach

Five small, additive UX changes — none change command behavior:

1. **CWD-aware `--help` banner.** When `hyalo --help` (and subcommand help)
   runs, prepend a one-line notice via clap's `before_help` /
   `before_long_help` if either:
   - cwd contains a `.hyalo.toml` → "ℹ️ hyalo runs against `<dir>` (from
     ./.hyalo.toml). Don't `cd` into it; pass paths relative to `<dir>`."
   - cwd is inside the configured `dir` and `.hyalo.toml` lives in an
     ancestor → "⚠️ You are inside the kb folder. Run hyalo from
     `<repo-root>` instead — `dir` is auto-resolved from .hyalo.toml."
   - otherwise: no banner (don't pollute help in unrelated projects).

   The banner string is computed at `Command` construction time, since
   clap's help is declarative.

2. **Redundant `--dir` warning.** When the user passes `--dir <path>` and
   the canonical path equals the `dir` already resolved from `.hyalo.toml`,
   emit a one-line stderr note: `note: --dir is redundant; .hyalo.toml
   already sets dir = "<dir>"`. Fires at most once per invocation, same
   `OnceCell` pattern as iter-128.

3. **`hyalo config` subcommand.** Prints:
   - path to the resolved `.hyalo.toml` (or "<none>" if not found)
   - the raw file contents (when present)
   - the resolved effective values: `dir`, `cwd`, plus any other settings
     surfaced by the config loader

   Default text format; `--format json` returns the same as a JSON object
   (so agents can parse it). Read-only — does not mutate config.

4. **Resolved `dir` in `--version`.** Append `(kb dir: <dir>)` to the
   `--version` output when `.hyalo.toml` is found, otherwise leave the
   plain version string. Cheap, agent-visible.

5. **Resolved `dir` in `hyalo summary` header.** Add a `kb dir: <dir>`
   line to the summary output (text format) and a `dir` field (JSON
   format).

Skipped from the brainstorm: refusing mutating commands when cwd == vault
dir (too aggressive — warning is enough, iter-128 already covers it).

## Tasks

- [x] Add a `cwd_help_banner()` helper in the CLI module that returns
      `Option<String>` based on cwd vs. resolved `.hyalo.toml`/`dir`
- [x] Wire the banner into the root `Command` via `before_help`
      (and `before_long_help`) so it appears for `hyalo --help` and
      subcommand help
- [x] Detect redundant `--dir` after config load; emit one-time stderr
      note using the existing once-per-invocation warning helper from
      iter-128 (extend or reuse)
- [x] Implement `hyalo config` subcommand: text + JSON output, includes
      config file path, raw contents, and resolved effective values
- [x] Update `--version` formatting to append `(kb dir: <dir>)` when a
      `.hyalo.toml` is resolved
- [x] Add `kb dir:` line to `hyalo summary` text output and `dir` field
      to JSON output
- [x] Unit tests: banner returns expected string for each cwd condition
      (in vault, at repo root, unrelated dir)
- [x] Unit tests: redundant `--dir` warning fires once and only when
      paths canonicalize equal
- [x] e2e test: `hyalo --help` from a dir with `.hyalo.toml` shows the
      banner; from an unrelated dir shows no banner
- [x] e2e test: `hyalo config` prints expected fields in text and JSON
- [x] e2e test: `hyalo --version` includes `(kb dir: ...)` when run in
      the kb workspace and omits it elsewhere
- [x] e2e test: `hyalo summary` includes `kb dir:` / `dir` field
- [x] Update README + `.claude/CLAUDE.md` + hyalo skill to mention
      `hyalo config` and the CWD-aware help banner

## Acceptance Criteria

- [x] `hyalo --help` shows a CWD-aware banner when run from a project
      with `.hyalo.toml`, and no banner from unrelated dirs
- [x] `hyalo --dir <same-as-config>` emits a one-time `note: --dir is
      redundant` to stderr and otherwise behaves identically
- [x] `hyalo config` prints config path, raw contents, and resolved
      effective values; supports `--format json`
- [x] `hyalo --version` includes `(kb dir: <dir>)` when `.hyalo.toml`
      is resolved, plain version otherwise
- [x] `hyalo summary` output (text + JSON) exposes the resolved `dir`
- [x] No existing tests regress; new unit + e2e tests cover all five
      changes
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean
