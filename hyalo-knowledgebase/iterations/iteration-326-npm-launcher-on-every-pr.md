---
title: "Iteration 326: npm-launcher on every PR"
type: iteration
date: 2026-11-06
tags: [iteration, ci, gates]
status: in-progress
branch: iter-326/npm-launcher-on-every-pr
priority: 1
related: ["[[decision-log]]", "[[iterations/iteration-323-index-parity-robustness]]"]
---

# npm-launcher on every PR

## Problem

`npm-launcher` is the only CI job that runs `npm test`, and it was not a required check, so GitHub would have merged a failing run. It could not simply be required, because the `paths:` filter in `npm-packages.yml` means it never starts on some PRs, and a required check that never starts blocks the merge for good. See DEC-398.

## Tasks

- [x] Remove the `paths:` filter from `.github/workflows/npm-packages.yml`
- [x] Record the decision and the required order as DEC-398
- [ ] Merge this PR with `npm-launcher` green on all three platforms
- [ ] Add the three `npm-launcher` checks to the `main` ruleset
- [ ] Confirm a PR outside the old filter reports all three checks
