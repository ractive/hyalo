---
title: Iteration 170 — GitHub annotations output for lint + lint own KB in CI
type: iteration
date: 2026-07-17
tags:
  - iteration
  - lint
  - ci
  - github-actions
status: planned
branch: iter-170/lint-github-annotations
---

# Iteration 170 — GitHub annotations output for lint + lint own KB in CI

## Goal

Make `hyalo lint` a first-class PR check on GitHub: violations surface as inline
annotations on the PR diff via GitHub Actions workflow commands, and the hyalo
repo dogfoods this by linting `hyalo-knowledgebase/` in its own CI. This is the
Rust half of the GitHub Action story; the distribution half (setup action) is
[[iteration-171-setup-hyalo-action]].

## Background

`hyalo lint` already has CI-grade exit codes (0 clean / 1 errors / 2 internal),
deterministic JSON output, `--strict`, and `--files-from -` for diff-aware runs
(see research 2026-07-17). What's missing is native
`::error file=…,line=…::message` output so findings render as PR annotations
without jq glue — which would violate the no-polyglot-tooling rule anyway.

## Tasks

### 1. `--format github` for lint

- [ ] Accept a `github` output format for `hyalo lint` (and `lint --fix --dry-run`), emitting one GitHub Actions workflow command per violation: `::error file=<path>,line=<line>,title=<RULE_ID>::<message>` (warnings → `::warning`)
- [ ] Decide and document scope: `github` is lint-only — other subcommands reject it with a clear error listing valid formats
- [ ] Emit paths **relative to the repository root** (annotations resolve against the workspace, not the vault dir): prefix vault-relative paths with the vault dir's path relative to CWD; document the assumption that CI runs from the repo root
- [ ] Escape message data per the workflow-command spec (`%` → `%25`, `\r` → `%0D`, `\n` → `%0A`; in properties also `:` → `%3A`, `,` → `%2C`)
- [ ] `--strict` promotion, `--rule`/`--rule-prefix`, `--limit`, and `[lint] ignore` all compose with the new format unchanged; exit codes unchanged
- [ ] After annotations, print a one-line summary to stdout (`N errors, M warnings in K files`) so the job log stays readable

### 2. Dogfood: lint the knowledgebase in CI

- [ ] Add a `lint-kb` job to `.github/workflows/ci.yml`: build `hyalo-cli` (or reuse the test job's build cache) and run `hyalo lint --strict --format github` against `hyalo-knowledgebase/`
- [ ] Fix (or explicitly waive via `[lint] ignore` / rule config) any violations the new gate surfaces in the existing KB, in the same PR
- [ ] Job runs on ubuntu only (annotations are platform-independent)

### 3. Tests

- [ ] Unit: workflow-command escaping (all special chars, multi-line messages)
- [ ] Unit: path prefixing when vault dir ≠ CWD (`--dir sub/kb`), including `.` vault
- [ ] e2e: `lint --format github` on a fixture vault with errors + warnings → exact expected `::error`/`::warning` lines, exit code 1; clean vault → summary only, exit 0
- [ ] e2e: `--strict` flips missing-type/undeclared-property annotations from `::warning` to `::error`
- [ ] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 4. Docs sync (same PR)

- [ ] README: new "CI / PR checks" section with a copy-paste workflow snippet (checkout → install → `hyalo lint --strict --format github`), incl. the diff-aware `--files-from -` variant
- [ ] Document the reserved-file drift check for OKF vaults in the same snippet: `hyalo okf index` (dry-run by default, non-zero exit on drift — landed in iter-165) as an optional second CI step
- [ ] `hyalo lint --help` documents the `github` format and repo-root path behavior
- [ ] Knowledgebase: record the format decision in the decision log

## Acceptance criteria

- [ ] A GitHub Actions job running `hyalo lint --strict --format github` on a vault with violations fails the check and shows inline annotations on the PR diff at the right file/line
- [ ] Clean vault → green check, no annotations
- [ ] hyalo's own CI lints `hyalo-knowledgebase/` and is green on main
- [ ] No output change for existing `text`/`json` formats
