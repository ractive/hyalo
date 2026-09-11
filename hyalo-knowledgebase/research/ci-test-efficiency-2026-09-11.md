---
type: research
title: CI and test efficiency audit
date: 2026-09-11
status: completed
---

# CI and test efficiency audit

Scope: the three validation workflows after iterations 290–295, at
`87db29fda7d88b23f25d136b8d371c6633c82bcf`. This is a bounded audit of five
candidates, not an inventory of the full test suite. No test definitions were
deleted. Release workflows, product behavior and branch rules are unchanged.

## Changes and retained coverage

1. **Automatic scale matrix:** remove the three benchmark jobs from ordinary
   PRs and pushes to main. Keep all three native platforms when the PR carries
   `bench-scale`, including immediately when that label is applied, or when
   Benchmarks is manually dispatched. A separate `benchmarks.yml` contains
   only benchmark jobs, so these events cannot replace correctness evidence
   with skipped checks. Other label events do not rerun benchmarks.
   Each benchmark still passes the native host explicitly and executes the
   Cargo-reported artifact. `CARGO_TARGET_DIR=target/scale` remains nondefault;
   rust-cache now caches that same directory with `. -> target/scale`.
2. **Native case-policy rerun:** exclude `mv_batch_equivalent_names` from the
   preceding workspace test invocation. The focused `--nocapture` step still
   runs both `follow_existing_destination_case_policy` and
   `follow_new_destination_case_policy` tests once on Linux, macOS and Windows.
   Their filesystem probes, collision/no-data-loss assertions, parent cleanup
   assertions and `HYALO_CASE_MATRIX` branch markers are unchanged.
3. **Separate Linux npm packaging job:** fold it into the Ubuntu launcher job,
   reusing its same-commit release binary, installed dependencies and API dist.
   Keep native launcher/API tests on all three OS, generation drift, all eight
   staged packages, tarball inventories, offline dry-run publication and clean
   consumer CLI/API checks. Remove the extra `cargo test -p xtask npm_package`:
   those unignored unit tests already execute in each native workspace suite.
4. **Bundled-skill Cargo startup:** resolve/build the CLI once using the existing
   Cargo artifact resolver, then execute each scratch-vault init/lint directly.
   Every shipped template and Pi/Codex skill still gets a separate initialized
   vault and its profile validation. This removes repeated Cargo invocations,
   not lint cases or assertions.
5. **MutationJournal source guard:** retain it. Although its forbidden helper
   names originated in a migration, Rust types do not prevent a command from
   bypassing journal/index maintenance. It also guards current direct
   persistence calls. Stability alone is not evidence that it is obsolete.

Typed-output, resource/confinement, partial-effects, index, recipe, generation
and package contracts remain automatic. Quality Gates retains all eleven
checks as a PR-only job inside CI, retaining its `quality-gates` job name and
inheriting `contents: read`. The former quality-gates workflow is removed,
leaving three validation workflows: CI, npm Packages and Benchmarks. No privileged PR
trigger, secrets, artifact handoff or publishing permission was introduced.

## Cost evidence and limits

From the workflow diff: each ordinary PR and each main push avoids three
native scale executions and their isolated builds. Each matching npm PR avoids
one Linux release build environment/cache restore, one dependency install,
one API typecheck/build, and one duplicate filtered xtask test invocation.
The two native move cases now execute once instead of twice per OS. The
bundled-skills helper reduces inner Cargo invocations from twice the number of
skills to one build; it still executes the same number of CLI commands.
These are operation counts, not measured runner-minute or cache-hit savings.
No cross-workflow binary sharing was introduced; Quality Gates still builds
its release binary for recipe execution.

Linux packaging now reports inside `npm-launcher (ubuntu-latest)` rather than
a separate `npm-packaging` check. At audit time, GitHub's effective main-branch
rules endpoint returned `[]`; legacy branch protection returned HTTP 404 with
"Branch not protected". Active ruleset 14172286 contains only `deletion` and
`non_fast_forward` rules, with empty ref includes/excludes; it contains no
required status checks. These reads provide no evidence that `npm-packaging`
is required; no rule was changed.
Hosted Linux packaging,
Windows behavior and benchmark cache hits still require CI observation.

The cache action treats target paths as workspace-relative in its pinned
implementation, so an absolute runner-temp path was not substituted blindly.
See [rust-cache configuration](https://github.com/Swatinem/rust-cache/blob/c19371144df3bb44fab255c43d04cbc2ab54d1c4/src/config.ts)
and [GitHub PR events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request).

## Validation

Local focused validation is recorded in the implementation handoff. The parent
workflow will run the mandatory fmt, workspace clippy and workspace tests in
order before committing. No full suite or benchmark campaign was repeated
for this audit.

The research note was scaffolded and its metadata set through Hyalo; prose
was appended directly because Hyalo has no general body-writing command.

Focused checks passed: actionlint for all three workflows, `cargo fmt --all
--check`, `git diff --check`, and the actual `check-bundled-skills` gate on
macOS (all 14 skills passed). That gate now makes one inner Cargo build rather
than 28 Cargo runs. Hyalo strict lint of this note reported zero warnings and
errors. Logs are in `/tmp/hyalo-ci-efficiency/` in the implementation workspace.
