---
title: Iteration 186 — diff-aware KB lint in CI (annotation budget)
type: iteration
date: 2026-07-19
tags:
  - iteration
  - ci
  - lint
  - github-actions
status: planned
branch: iter-186/diff-aware-lint-ci
---

# Iteration 186 — diff-aware KB lint in CI (annotation budget)

## Goal

Make PR lint annotations useful again: the `lint-kb` job lints only the
markdown files a PR actually touches, so GitHub's per-step annotation cap
(10 warnings + 10 errors per step) is spent on the PR's own findings —
while full-vault enforcement moves to a job that still runs on every
merge. Plus a small `--format github` honesty upgrade in hyalo itself:
deterministic annotation order and a truncation notice when counts exceed
what GitHub will register.

## Background

- Discovered closing iteration-171 (PR #209): the vault emits 668 MD013
  warnings and GitHub registers only 10 warning annotations per step, in
  parallel/nondeterministic emission order — the deliberately-seeded
  fixture warning never got registered because `decision-log.md` exhausted
  the cap first. See [[iteration-171-setup-hyalo-action]] "Update
  (2026-07-18, post-v0.18.0)".
- Iter-170 lifted *hyalo's* output caps for `--format github` ("no
  annotation silently dropped") — but GitHub drops them instead; hyalo
  neither orders them deterministically nor tells the reader truncation
  happened.
- The building blocks already exist: `--files-from -` (walk bypass,
  vault-prefix stripping, non-md inputs skipped), iter-174 skip-summary
  notices for dropped input paths, and the README already documents a
  diff-aware recipe (`git diff --name-only origin/main -- '**/*.md' |
  hyalo lint --strict --files-from - --format github`) in three places
  (CI section, okf profile, changelog profile).
- **CI trigger reality**: `.github/workflows/ci.yml` runs on
  `pull_request` only — there is no push-to-main or scheduled run. Today
  the PR job IS the only full-vault check, so going diff-aware on PRs
  without adding a full-vault job elsewhere would leave cross-file
  regressions (deleting a file others link to, schema changes) entirely
  unchecked.

## Tasks

### 1. CI: split lint-kb into diff-aware (PR) + full (main)

- [ ] `lint-kb` (pull_request): checkout with enough history for a
  merge-base diff (`fetch-depth: 0`, or depth-1 plus an explicit
  `git fetch origin $GITHUB_BASE_REF`), then
  `git diff --name-only --diff-filter=d origin/$GITHUB_BASE_REF...HEAD |
  hyalo lint --strict --files-from - --format github`
  (`--diff-filter=d` excludes deleted paths; keep `${{ github.base_ref }}`
  not hard-coded `main`)
- [ ] Decide two-dot vs three-dot and align the README: the landed snippet
  uses two-dot `origin/main` (compares the base *tip*, so a stale branch
  gets annotations for files it never touched); recommend three-dot
  (merge-base) for CI, and update all three README snippets to the same
  form in this PR — the repo must not carry two divergent diff-aware
  recipes (iter-171 alignment rule)
- [ ] Empty diff (docs-untouched PR): verify `--files-from -` with empty
  stdin exits 0 / "0 files checked" and the job stays green
- [ ] New `lint-kb-full` job on push to main: ci.yml gains a
  `push: branches: [main]` trigger scoped to this job (or a separate
  workflow file) running today's full
  `hyalo lint --strict --format github` — full-vault health enforced on
  every merge, where annotation caps don't matter
- [ ] Document the accepted gap in the job comment: a PR that breaks
  *other* files' links passes the diff-aware PR check and is caught by
  `lint-kb-full` on main — deliberate trade-off, annotations belong to
  the PR's own files

### 2. hyalo: `--format github` truncation honesty

- [ ] Deterministic emission order: sort annotations by (path, line,
  rule) before emitting — which findings GitHub registers under its cap
  must be stable across runs (root cause of the iter-171 evidence
  flakiness)
- [ ] Trailing `::notice::` summary whenever error count > 10 or warning
  count > 10 per invocation: state true totals and that GitHub registers
  at most 10 of each per step (skip when under the cap — quiet when
  nothing is hidden)
- [ ] README: document GitHub's 10-per-type-per-step cap in the
  `--format github` section next to the "no annotation silently dropped"
  claim (hyalo's side holds; GitHub's does not)
- [ ] e2e tests: ordering is sorted and stable; notice appears at >10 and
  not at ≤10; existing exit-code contract unchanged

### 3. Verification

- [ ] Dogfood PR: touch one KB file introducing a deliberate MD013
  warning → the PR check annotates exactly that file (registered
  annotation visible via the check-runs API), job green under `--strict`;
  remove the file before merge
- [ ] `lint-kb-full` observed green on main after this iteration's own
  merge
- [ ] Local: piping an empty list and a deleted-path list through
  `--files-from -` behaves per iter-174 notices

## Acceptance criteria

- [ ] A PR touching k markdown files lints exactly those k files in the
  `lint-kb` PR check; annotations are deterministic and belong to the PR
- [ ] Full-vault `--strict` lint runs on every push to main and fails on
  vault-wide regressions
- [ ] `--format github` output is sorted (path, line, rule) and emits a
  truncation `::notice::` only when GitHub's per-type cap is exceeded
- [ ] README carries exactly one diff-aware recipe form, matching what CI
  runs, in all three places it appears

## Notes

- Independent of the 183→184→185 link chain — can run before, after, or
  between them.
- Out of scope: annotation *budgeting* beyond the notice (e.g. picking
  which 10 warnings to surface), job-summary markdown reports
  (`$GITHUB_STEP_SUMMARY`) — candidate follow-up if the notice proves
  insufficient.
