---
title: Iteration 171 — setup-hyalo GitHub Action + agent-on-GitHub recipe
type: iteration
date: 2026-07-17
tags:
  - iteration
  - ci
  - github-actions
  - distribution
status: in-progress
branch: iter-171/setup-hyalo-action
---

# Iteration 171 — setup-hyalo GitHub Action + agent-on-GitHub recipe

## Goal

Ship `ractive/setup-hyalo@v1`: a composite GitHub Action that installs the
prebuilt hyalo binary on any runner in seconds, so (a) any repo can add a
`hyalo lint` PR check with two workflow steps, and (b) `claude-code-action`
agents on GitHub get hyalo on `PATH` and can use the committed hyalo/OKF skill.
Depends on [[iteration-170-lint-github-annotations]] for the annotations format
used in the examples.

## Background

Release artifacts already exist per platform on GitHub releases
(`hyalo-VERSION-<target>.tar.gz` / `.zip` for windows) via
`ractive/release-workflows`; there is no action.yml, Docker image, or
install.sh today (research 2026-07-17). A separate action repo (the
`dtolnay/rust-toolchain` pattern) decouples action versioning from binary
versioning and allows a floating `@v1` tag. Skills land in consumer repos via
`hyalo init --claude` (OKF variant since iter-164).

## Tasks

### 1. `ractive/setup-hyalo` repo (new, separate repo)

- [x] `action.yml` composite action; inputs: `version` (default `latest`), `github-token` (default `${{ github.token }}`, for release-asset API rate limits)
- [x] Resolve runner platform → release asset name: linux/macos x86_64+aarch64 (`.tar.gz`), windows x86_64+aarch64 (`.zip`); fail with a clear error on unsupported platforms
- [x] `version: latest` resolves via the GitHub releases API; explicit `version: vX.Y.Z` downloads that tag; validate the input format
- [x] Cache the extracted binary in the tool cache keyed by version + platform; add to `GITHUB_PATH`
- [x] Implementation stays shell (bash) + `gh`/`curl` inside the composite steps — no Node/Python (consistent with the no-polyglot rule); Windows steps use bash (available on all GitHub-hosted runners)
- [x] Output: `version` (resolved version), and a post-install `hyalo --version` sanity step
- [x] Smoke-test workflow in the action repo: matrix over ubuntu/macos/windows × `latest`/pinned version → `hyalo --version` + `hyalo lint` on a tiny fixture vault
- [x] MIT license, README with usage, pin-by-SHA guidance

### 2. Versioning & release protocol

- [ ] Tag `v1` (floating) + `v1.0.0` on the action repo; document the retag protocol in the action README
- [ ] Decide + document whether the hyalo release pipeline should smoke-test `setup-hyalo` with each new release (follow the ractive/release-workflows change protocol from the KB); no automation required this iteration — a documented manual step is enough

### 3. Consumer recipes (docs, in the hyalo repo)

- [x] README "CI / PR checks" section (from iter-170) upgraded to use `ractive/setup-hyalo@v1` instead of manual download; full snippet: checkout → setup-hyalo → `hyalo lint --strict --format github`
- [x] Diff-aware variant documented — reuse the exact snippet iter-170 already shipped in the README rather than a fresh one: `git diff --name-only origin/main -- '**/*.md' | hyalo lint --strict --files-from - --format github` (note: **not** `origin/main...HEAD -- '*.md'` — align on the landed syntax so the repo doesn't carry two slightly different diff-aware examples)
- [x] Agent recipe documented: `claude-code-action` workflow with a preceding `setup-hyalo` step, `allowed_tools: Bash(hyalo:*)`, and the repo carrying the skill from `hyalo init --claude` — so `@claude` mentions can triage/fix lint findings with `hyalo set` / `lint --fix`
- [x] If the agent recipe demonstrates `hyalo lint --fix` (mutating or `--dry-run`) combined with `--format github`, sanity-check against a file with an unfixable violation (e.g. a missing required property) — iter-170's PR review found the fix-mode output path uses a different JSON shape (`remaining_groups`) than read-only lint (`rule_groups`), which the github-format renderer initially missed entirely; that's fixed on main now, but any *new* lint output consumer should assume both shapes exist and test both, not just the read-only path
- [ ] Convert hyalo's own `lint-kb` CI job (iter-170) to use the published action — end-to-end dogfood of the release artifact path

### 4. Verification

- [ ] Smoke matrix in the action repo green on all three OSes
- [ ] A real PR in the hyalo repo shows inline lint annotations produced via the action
- [x] Knowledgebase: record the separate-repo + floating-tag decision in the decision log

## Acceptance criteria

- [ ] `uses: ractive/setup-hyalo@v1` followed by `hyalo lint --strict --format github` is a working two-step PR check on ubuntu, macos, and windows runners
- [ ] Pinned `version:` input installs exactly that release; `latest` tracks the newest
- [ ] hyalo's own CI uses the action for its KB lint job
- [x] README documents both the PR-check and the claude-code-action recipes

## Status (2026-07-17)

**Landed in the hyalo repo (this PR):**

- Full `ractive/setup-hyalo` action tree built and **staged** under
  `research/setup-hyalo-action/` (see its `PUBLISH.md`): `action.yml` (composite
  bash, `version` + `github-token` inputs, platform→target resolution, tool-cache
  keyed by version+platform, `GITHUB_PATH`, `version` output + `hyalo --version`
  sanity step), matrix `smoke.yml` (ubuntu/macos-14/windows × latest/pinned →
  `hyalo --version` + `hyalo lint` on a fixture vault), MIT `LICENSE`, README
  (usage, inputs/outputs, pin-by-SHA, retag protocol, manual release smoke test).
- Install logic **verified end-to-end on macOS bash 3.2**: `latest` + pinned +
  warm-cache paths, and version-input rejection. Two portability bugs found and
  fixed: empty-array expansion under `set -u` (bash 3.2), and a `pipefail` +
  `grep -m1` broken-pipe abort — both would have broken the action on real
  runners.
- README upgraded: "GitHub PR annotations" workflow now uses
  `ractive/setup-hyalo@v1`; new `@claude` agent recipe (`claude-code-action` +
  `setup-hyalo` + `allowed_tools: Bash(hyalo:*)`); fix-mode `--format github`
  behavior verified against an unfixable missing-property violation.
- Decision recorded: [[decision-log#DEC-051]] (separate repo + floating `@v1`).

**Blocked — needs a human:**

- `gh repo create ractive/setup-hyalo` (public) is denied to the automated run;
  creating a new public repo requires web-UI authorization. Publish via the steps
  in `research/setup-hyalo-action/PUBLISH.md`, then tag `v1.0.0` + `v1`.
- hyalo's own `lint-kb` CI job is **intentionally left on build-from-source** —
  pointing live CI at the not-yet-published `ractive/setup-hyalo@v1` would break
  every PR check. Flip it after the action repo is published.
- Smoke-matrix green + a real annotated PR are verifiable only once the repo is
  public.
