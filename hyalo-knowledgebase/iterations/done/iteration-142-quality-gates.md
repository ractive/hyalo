---
title: "Iteration 142 — Quality gates: AC-fidelity, feature-fanout matrix, help-code drift"
type: iteration
date: 2026-05-24
status: completed
branch: iter-142/quality-gates
tags:
  - iteration
  - quality
  - ci
  - help-text
  - llm-ergonomics
  - acceptance-criteria
related:
  - "[[dogfood-results/dogfood-v0160-deep]]"
  - "[[iteration-138-schema-extensions-and-new-command]]"
  - "[[iteration-139-files-from-flag]]"
  - "[[iteration-140-dogfood-138-139-fixes]]"
  - "[[iteration-141-dogfood-v0160-fixes]]"
---

## Goal

Three mechanical guards that catch — at PR time, not at dogfood time —
the categories of bug that keep recurring across dogfood sessions:

1. **AC fidelity** — every ticked `## Acceptance criteria` checkbox in
   an iteration plan must have a corresponding test reference or
   explicit deferral annotation. Catches partially-wired features
   (iter-138's `required_sections` lint dead code shipped with the AC
   ticked because there was a unit test but no e2e).
2. **Feature-fanout matrix** — when a flag is added to one subcommand,
   declare which siblings must also implement it; one e2e test walks
   the matrix and asserts every cell behaves consistently. Catches
   cross-command inconsistency (iter-139's `--files-from` counters at
   top-level on `lint` but missing from `find`).
3. **Help-code drift CI check** — every clap command must have an
   `EXAMPLES:` block with ≥2 examples; help text must not contain
   known-stale wordings; every documented flag must exist. Catches
   docs that drift behind code (iter-140 fixed `create_dir_all` but
   `new --help` still said "parent must exist" in v0.16.0).

Scope is intentionally **mechanism, not content**. We are building the
guards, not the long tail of new examples or fixture vaults — those
land in follow-up iterations (likely iter-142b for content).

Background and motivation: this is the proposal from the
[[dogfood-v0160-deep]] follow-up discussion. The pattern across iter-138
→ iter-139 → iter-140 → iter-141 is "each new feature has a partial
implementation that ships with ACs ticked, and dogfood catches it 1–2
iterations later". The cost of each catch is a full ralph-loop +
dogfood cycle. The proposal is to move the catches before merge.

## Pre-work

The ralph-loop SKILL.md references `crates/xtask`,
`check-dead-primitives`, and `check-todo-annotations`. **None of these
exist in the current repo**; the SKILL.md is aspirational. iter-142
also stands up the xtask infrastructure so future quality-gate work
can land in the same place.

- [x] Add `crates/xtask/` workspace member with `Cargo.toml`,
      `src/main.rs`, and a `cargo run -p xtask -- <subcommand>` entry
      point.
- [x] Subcommand stubs (no-op exit-0) for the historic SKILL.md names:
      `check-dead-primitives`, `check-todo-annotations`. Implementing
      these is **out of scope** for iter-142; the stubs let the
      ralph-loop skill stop warning about missing checks. Add a
      `// allow-todo: iter-142b` comment in each stub.
- [x] CI workflow file `.github/workflows/quality-gates.yml` that runs
      the three new checks below on every PR. Reuses the existing
      Rust toolchain matrix.

## Issues

### Gate 1 — AC fidelity check (HIGH leverage)

`cargo run -p xtask -- check-ac-fidelity --plan <path>` (and the
`--since <ref>` form for CI).

For each iteration plan file (`hyalo-knowledgebase/iterations/**/*.md`):

1. Parse the `## Acceptance criteria` section. Collect every line
   matching `^- \[(x| )\] (.+)$`; ticked items are obligations.
2. For each ticked AC, search the iteration's branch diff (or `HEAD`
   tree if `--plan` was given directly) for evidence:
   - A new or modified test file whose name or body references a
     keyword from the AC text (loose substring match, normalized).
   - An explicit deferral annotation in the plan: a child bullet under
     the AC saying `[deferred — new plan: iter-NNN]` (the bracket form
     is the contract).
3. Exit 1 if any ticked AC has no evidence and no deferral.

- [x] Parser for `## Acceptance criteria` (reuse the existing markdown
      machinery in `hyalo-core`; this is the same kind of section walk
      that iter-138's `validate_required_sections` does).
- [x] Evidence-search via `git diff --name-only origin/main..HEAD` +
      file content scan. Avoid loading whole files when possible —
      grep-style line-by-line.
- [x] Deferral grammar: a child bullet `[deferred — new plan: iter-NNN]`
      where `iter-NNN` must match an existing plan file (or `iter-???`
      for a placeholder slot to be created).
- [x] Friendly error: which AC, which plan, what was searched for,
      and the deferral grammar reminder.
- [x] E2E test: synthetic plan with ticked AC and matching test
      keyword → exit 0. Same without the test → exit 1. With a
      deferral annotation → exit 0.
- [x] **Self-check**: run on iter-138/139/140/141 retrospectively;
      document which historic ACs would have tripped. (Don't fail CI
      for historic plans; gate fires on iter-142+.)

### Gate 2 — Feature-fanout matrix (HIGH leverage)

`cargo run -p xtask -- check-feature-fanout` plus a Rust e2e test
that exercises the matrix.

The matrix is **explicit**, encoded in `crates/xtask/feature-matrix.toml`:

```toml
# Flags that should fan out across multiple subcommands.
[flags."--files-from"]
required_in = ["find", "lint", "mv", "set", "remove", "append"]
shape = "selector"  # selects which files the command operates on

[flags."--glob"]
required_in = ["find", "lint", "mv", "set", "remove", "append"]
shape = "selector"

[flags."--index-file"]
required_in = ["find", "lint", "summary", "backlinks", "links",
               "tags", "properties"]
shape = "read-source"

# Envelope-shape contracts.
[envelopes]
# Commands listed below MUST surface these counters under .results when
# --files-from is in effect.
files_from_counters = ["find", "lint", "mv", "set", "remove", "append"]
```

Two enforcement layers:

**Static**: `check-feature-fanout` parses the clap command tree (via
`hyalo --help <cmd>` for each cmd in `required_in`) and asserts the
flag is present. Exit 1 if a documented cell is missing.

**Runtime**: `crates/hyalo-cli/tests/e2e/feature_matrix.rs` runs a
single `--files-from -` invocation per cell with a fixture vault and
asserts the JSON envelope has the expected counter keys under
`.results` (no top-level escape, no array shape).

- [x] `crates/xtask/feature-matrix.toml` with the cells listed above
      + any others the team identifies during review.
- [x] `check-feature-fanout` xtask: parse the matrix, parse `--help`,
      diff and report.
- [x] `feature_matrix.rs` e2e test in `hyalo-cli/tests/e2e/` that
      walks the runtime side of the matrix.
- [x] Fixture vault: a tiny `tests/fixtures/feature-matrix-vault/`
      with one note file per type so each cell has something real to
      operate on. **Reuse if a similar fixture already exists**;
      don't grow the test corpus needlessly.
- [x] When CI fails on a missing cell, the error must say exactly
      which flag, which command, and what the matrix file says — so
      the fix is obvious from the failure message alone.

### Gate 3 — Help-code drift CI check (MEDIUM leverage, low cost)

`cargo run -p xtask -- check-help-drift`.

Two static checks against the clap tree:

**3a. EXAMPLES block required.** Every top-level subcommand and every
nested subcommand must have an `EXAMPLES:` (or `Examples:`) section
in its `long_about` containing at least two example invocations
(lines starting with `  hyalo ` or `  $ hyalo `, or fenced shell
blocks containing the same).

- [x] Walk `clap::Command::get_subcommands()` recursively.
- [x] Parse `long_about` for an EXAMPLES section.
- [x] Count examples; require ≥2 per command. Fail with the offending
      command name(s).
- [x] Allowlist for genuinely no-op commands (`help`, `completion`):
      explicit list, not a regex.

**3b. Stale-wording grep.** Maintain a small file
`crates/xtask/stale-help-patterns.toml` of phrases that no longer
match behaviour:

```toml
patterns = [
  { pattern = "parent must exist",
    reason = "iter-140 fixed via create_dir_all" },
  { pattern = "parent directory does not exist",
    reason = "iter-140 fixed via create_dir_all" },
]
```

Each entry has an explicit reason. When a real change reintroduces
the wording, the fix is either to update the code, update the help,
or remove the pattern from the file with a justification.

- [x] Implement the grep over every clap `long_about` and `help`
      string.
- [x] Fail with line-and-command pointing to the offending phrase and
      its recorded reason.
- [x] Seed the file with the three known iter-140/141 phrases.

## Tasks

- [x] Pre-work: stand up `crates/xtask/` + workspace + Cargo.toml
- [x] Pre-work: stub `check-dead-primitives`, `check-todo-annotations`
- [x] Pre-work: `.github/workflows/quality-gates.yml`
- [x] Gate 1: AC-section parser
- [x] Gate 1: evidence-search via `git diff --name-only` + content scan
- [x] Gate 1: deferral grammar + plan-file cross-ref
- [x] Gate 1: friendly errors
- [x] Gate 1: e2e tests (ticked + evidence; ticked + no evidence;
  ticked + deferral)
- [x] Gate 1: retrospective self-check on iter-138..141 documented in
  the PR description
- [x] Gate 2: `feature-matrix.toml` with documented cells
- [x] Gate 2: `check-feature-fanout` xtask
- [x] Gate 2: `feature_matrix.rs` e2e walker
- [x] Gate 2: fixture vault (reuse if available)
- [x] Gate 2: failure-message clarity
- [x] Gate 3a: `long_about` walk + EXAMPLES counter
- [x] Gate 3a: allowlist for no-op commands
- [x] Gate 3b: `stale-help-patterns.toml` with iter-140/141 seed
- [x] Gate 3b: grep + line-pointing errors
- [x] All three gates wired into `.github/workflows/quality-gates.yml`
- [x] CHANGELOG `Unreleased` entry under Added
- [x] Update ralph-loop SKILL.md notes about the new gates (the file
  is in `~/.claude/skills/ralph-loop/` not the repo, so this is a
  callout in the PR description rather than a file change)

## Acceptance criteria

- [x] `cargo run -p xtask -- check-ac-fidelity --plan
      hyalo-knowledgebase/iterations/iteration-142-quality-gates.md`
      runs cleanly against this iteration's own plan (recursive
      self-check)
- [x] A synthetic plan with a ticked AC that has no test evidence and
      no deferral causes `check-ac-fidelity` to exit 1 with a clear
      message
- [x] `cargo run -p xtask -- check-feature-fanout` is green on the
      current `main` (since iter-140/141 already brought `--files-from`
      into consistency); removing one of the cells (e.g. deleting the
      `--files-from` arg from `find`) makes it exit 1
- [x] `cargo run -p xtask -- check-help-drift` is green on current
      `main` after iter-141's EXAMPLES sweep; reintroducing "parent
      must exist" anywhere in `args.rs` makes it exit 1
- [x] All three checks run in CI on every PR and block merge on
      failure
- [x] Each gate's failure message tells you what to fix without
      reading the xtask source
- [x] CHANGELOG `Unreleased` updated under Added
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` green on all three CI platforms

## Design notes

- **Why a TOML matrix instead of attribute macros.** A `#[fanout(...)]`
  proc-macro on each clap arg would auto-derive the matrix, but it
  buries the contract in the call site. The whole point of these
  gates is *visibility*: someone reading `feature-matrix.toml` should
  be able to answer "what flag exists on what command, and why?" at a
  glance. TOML wins on readability; the static check parsing is
  cheap.
- **Why AC evidence is loose substring matching.** A strict parser
  ("each AC must name a test file") would over-fit — half of these
  ACs are observed behaviour, not test files (e.g. "the canonical
  recipe in `README.md` works"). Loose grep + the explicit deferral
  escape hatch covers both. False positives are corrected by
  tightening the AC wording in the plan, which is a *good* outcome —
  it sharpens the plan.
- **Why we don't auto-fix.** All three gates report-and-fail. The
  pattern in this codebase is "human writes a small thing, tool
  verifies"; auto-generating EXAMPLES blocks would erode the LLM-
  ergonomics value of having a *real* example written by a human
  (or LLM) thinking about the command.
- **CI cost.** All three gates are pure local computation on already-
  built artefacts (`cargo run -p xtask -- ...` after the existing
  build job). No new toolchain, no new external service, no
  network. Roundtrip ≈ a few seconds in the worst case.

## Out of scope

- **Property-based / fuzz testing on input parsers** (item 4 in the
  dogfood follow-up proposal). Genuinely useful for `--files-from`,
  `--glob`, and frontmatter parsing — but it's a separate concern
  from the gate categories above, and the right home is a dedicated
  iter-142c or iter-143 specifically about fuzzing. Don't fold it in
  here.
- **The "no new iteration for partial implementation" process rule**
  (item 5 in the dogfood follow-up). Pure process; no code change.
  Document as a decision-log entry separately; not blocking iter-142.
- **Implementing `check-dead-primitives` and `check-todo-annotations`**
  properly. They are stubs here. Real implementations belong in
  iter-142b.
- **Backfilling EXAMPLES content** on every subcommand. iter-141
  added that. iter-142 only enforces the contract going forward.
- **Implementing AC fidelity for historic iterations.** The
  retrospective self-check is informational only; we don't gate CI
  on iter-138..141 retroactively. That conversation belongs in a
  cleanup iteration if we decide it's worth the churn.
