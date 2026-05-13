---
title: Iteration 128 — Detect and warn about LLM hyalo misuse
type: iteration
date: 2026-05-07
tags:
  - ux
  - llm
  - dx
  - validation
status: completed
branch: iter-128/llm-misuse-warning
---

## Goal

LLM-driven shells (Claude Code, etc.) frequently misuse the hyalo CLI in two
recurring ways:

1. They pass absolute paths to `--file`, e.g.
   `hyalo set --file /Users/me/proj/proj-knowledgebase/iter-17.md ...`, and hit
   the cryptic `file resolves outside vault boundary` error.
2. They `cd` into the configured knowledgebase directory before invoking
   hyalo, then pass paths relative to that subdir. This works silently today
   but reinforces a broken mental model — the next call from a sibling dir
   blows up.

The configured `dir` in `.hyalo.toml` already pins the vault root, so the
LLM never needs to navigate into it or pass absolute paths. We should detect
both anti-patterns and emit a single, prominent stderr warning that corrects
the LLM's understanding so it stops repeating the mistake.

## Approach

Emit warnings (not errors) to **stderr** so the message reaches both humans
and LLM-driven shells (Claude Code captures stderr alongside stdout). The
warning is always on — no suppression knob — and fires at most once per
invocation regardless of how many `--file` args trigger it.

Behavior changes:

- **Absolute path inside the vault**: warn, strip the canonical vault prefix,
  proceed with the command. Today this errors out, which is unhelpful when
  the LLM clearly identified the file it wanted.
- **Absolute path outside the vault**: keep the existing `OutsideVault`
  hard error.
- **CWD == vault dir or inside it** (when `.hyalo.toml` is in an ancestor):
  warn, proceed normally. The command still works; the warning just teaches
  the LLM not to `cd` next time.

Warning text should be short, blunt, and tell the LLM exactly what to do:

```text
warning: hyalo is configured with dir = "<dir>".
  Do not cd into "<dir>" or pass absolute paths to --file.
  Run hyalo from the project root and pass paths relative to "<dir>", e.g.
    hyalo set iterations/iteration-17.md --property status=in-progress
```

## Tasks

- [x] Detect absolute `--file` / positional paths in `discovery::resolve_file`; if the canonical path is inside the canonical vault, strip the prefix and continue (instead of returning `OutsideVault`)
- [x] Keep `OutsideVault` error when an absolute path canonicalizes outside the vault
- [x] Detect when CWD equals or is inside the canonical vault dir, given that `.hyalo.toml` was found in an ancestor of the vault — emit the warning once per invocation
- [x] Implement the warning helper that fires at most once per process run (e.g., `OnceCell`-gated emit on stderr)
- [x] Wire the trigger into the file-resolution path so absolute-path warnings fire from any subcommand using `--file`
- [x] Wire the CWD-in-vault check into CLI startup (after config load) so it fires for every subcommand, not just file-mutating ones
- [x] Unit tests: absolute path inside vault → warn + resolve correctly; absolute path outside vault → still errors; CWD inside vault → warn fires; CWD outside vault → no warn; warning fires only once when multiple `--file` args are given
- [x] e2e test: invoke a command from inside the configured `dir` and assert the warning appears on stderr with the expected text
- [x] e2e test: invoke a command with an absolute `--file` inside the vault from the project root and assert the warning + successful execution
- [x] Update CLI help text / README / `.claude/CLAUDE.md` if any guidance contradicts the new behavior
- [x] Audit the hyalo skill (`.claude/skills/hyalo/SKILL.md` and the synced copy under `~/.claude/skills/hyalo/`) — add an explicit "always run from the project root, never cd into the configured `dir`, never use absolute `--file` paths" rule with a worked example, so the skill teaches the same lesson the runtime warning does
- [x] Dogfood: run hyalo against `hyalo-knowledgebase` and a sibling KB with absolute paths, confirm warnings render readably and don't double-fire

## Acceptance Criteria

- [x] Absolute `--file` path that resolves inside the vault works and emits one warning to stderr
- [x] Absolute `--file` path that resolves outside the vault still hard-errors with `OutsideVault`
- [x] Running any hyalo subcommand from inside the configured `dir` emits one warning to stderr and otherwise behaves identically
- [x] Warning is emitted at most once per process invocation, even with multiple `--file` args or repeated triggers
- [x] Warning text references the actual configured `dir` value and gives a concrete corrective example
- [x] hyalo skill (project + global copies) explicitly tells the LLM to run from the project root and never cd into `dir` or use absolute paths
- [x] All existing tests pass; new unit + e2e tests cover the four cases above
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -q` all clean
