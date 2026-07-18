---
title: "Iteration 181 — write-path polish (set advisories, exit codes, scaffolds)"
type: iteration
date: 2026-07-18
status: planned
branch: iter-181/write-path-polish
tags: [iteration, cli, ux, schema]
related:
  - "[[dogfood-results/dogfood-v0180-final-pre-release]]"
---

# Iteration 181 — write-path polish

## Goal

Small consistency fixes on the mutation/query paths from
[[dogfood-results/dogfood-v0180-final-pre-release]] — none dangerous
alone, together they make the tool feel predictable for agents.

## Tasks

### 1. `set` advisory notes for enum/pattern violations (LOW)

- [ ] `set --property status=bogus` (enum) and a pattern-violating value
  emit the same advisory `note` that date violations already get; write
  still proceeds (lint remains the enforcement gate)

### 2. Exit-code contract (LOW)

- [ ] `--jq` with `--format text` exits 1 (user error) instead of 2
  (help defines 2 = internal error); audit dispatch for other user
  errors mapped to 2

### 3. `set` JSON response reflects coercion (LOW)

- [ ] `--property 'x=[a, b]'` echoes the coerced YAML list in the JSON
  response, not the raw input string

### 4. Scaffold validity (LOW)

- [ ] `new --type iteration` no longer scaffolds `branch: TBD` (violates
  the type's own `^iter-\d+[a-z]*/` pattern); scaffold placeholders must
  pass the type's schema, or the property is omitted for the user to fill

### 5. Query ergonomics (LOWs from own-KB agent)

- [ ] `--property 'p>=v'` on non-numeric/non-date values emits a note
  that the comparison is lexicographic
- [ ] Property-regex parse errors show the engine detail like `find -e`
  does (`title~=(` currently gives no caret/position)
- [ ] `mv A B` positional destination accepted as alias for `--to`
  (consistency with other positional-file commands) — or explicitly
  decide against and document why
- [ ] `changelog add --wrap <cols>` (or config) to wrap long messages for
  80-column files (recorded LOW from two dogfoods running)

### 6. Retrospective

- [ ] Update remaining planned iterations with anything learned; keep
  help texts and README in sync with every flag change in this PR

## Acceptance Criteria

- [ ] Each task has an e2e test locking the new behavior
- [ ] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean
