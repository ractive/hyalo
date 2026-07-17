---
title: Iteration 171 — setup-hyalo GitHub Action + agent-on-GitHub recipe
type: iteration
date: 2026-07-17
tags:
  - iteration
  - ci
  - github-actions
  - distribution
status: planned
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

- [ ] `action.yml` composite action; inputs: `version` (default `latest`), `github-token` (default `${{ github.token }}`, for release-asset API rate limits)
- [ ] Resolve runner platform → release asset name: linux/macos x86_64+aarch64 (`.tar.gz`), windows x86_64+aarch64 (`.zip`); fail with a clear error on unsupported platforms
- [ ] `version: latest` resolves via the GitHub releases API; explicit `version: vX.Y.Z` downloads that tag; validate the input format
- [ ] Cache the extracted binary in the tool cache keyed by version + platform; add to `GITHUB_PATH`
- [ ] Implementation stays shell (bash) + `gh`/`curl` inside the composite steps — no Node/Python (consistent with the no-polyglot rule); Windows steps use bash (available on all GitHub-hosted runners)
- [ ] Output: `version` (resolved version), and a post-install `hyalo --version` sanity step
- [ ] Smoke-test workflow in the action repo: matrix over ubuntu/macos/windows × `latest`/pinned version → `hyalo --version` + `hyalo lint` on a tiny fixture vault
- [ ] MIT license, README with usage, pin-by-SHA guidance

### 2. Versioning & release protocol

- [ ] Tag `v1` (floating) + `v1.0.0` on the action repo; document the retag protocol in the action README
- [ ] Decide + document whether the hyalo release pipeline should smoke-test `setup-hyalo` with each new release (follow the ractive/release-workflows change protocol from the KB); no automation required this iteration — a documented manual step is enough

### 3. Consumer recipes (docs, in the hyalo repo)

- [ ] README "CI / PR checks" section (from iter-170) upgraded to use `ractive/setup-hyalo@v1` instead of manual download; full snippet: checkout → setup-hyalo → `hyalo lint --strict --format github`
- [ ] Diff-aware variant documented: `git diff --name-only origin/main...HEAD -- '*.md' | hyalo lint --files-from - --strict --format github`
- [ ] Agent recipe documented: `claude-code-action` workflow with a preceding `setup-hyalo` step, `allowed_tools: Bash(hyalo:*)`, and the repo carrying the skill from `hyalo init --claude` — so `@claude` mentions can triage/fix lint findings with `hyalo set` / `lint --fix`
- [ ] Convert hyalo's own `lint-kb` CI job (iter-170) to use the published action — end-to-end dogfood of the release artifact path

### 4. Verification

- [ ] Smoke matrix in the action repo green on all three OSes
- [ ] A real PR in the hyalo repo shows inline lint annotations produced via the action
- [ ] Knowledgebase: record the separate-repo + floating-tag decision in the decision log

## Acceptance criteria

- [ ] `uses: ractive/setup-hyalo@v1` followed by `hyalo lint --strict --format github` is a working two-step PR check on ubuntu, macos, and windows runners
- [ ] Pinned `version:` input installs exactly that release; `latest` tracks the newest
- [ ] hyalo's own CI uses the action for its KB lint job
- [ ] README documents both the PR-check and the claude-code-action recipes
