---
branch: iter-31/homebrew-distribution
date: 2026-03-23
status: completed
tags:
- iteration
- distribution
- homebrew
title: 'Iteration 31: Homebrew Tap & Distribution'
type: iteration
---

# Iteration 31: Homebrew Tap & Distribution

## Goal

Set up a Homebrew tap so macOS (and Linux Homebrew) users can `brew install ractive/tap/hyalo`. Auto-update the formula on each GitHub Release.

## Tasks

### Create Homebrew tap repository
- [x] Create `ractive/homebrew-tap` repository on GitHub
- [x] Add initial `Formula/hyalo.rb` with placeholder version
- [x] Formula should download the correct binary for the platform (macOS ARM64, macOS x86_64, Linux x86_64)
- [x] Include SHA256 checksums (from the SHA256SUMS artifact added in iter-25)

### Auto-update workflow
- [x] Add a job to the release workflow (`release.yml`) that updates the Homebrew formula after binaries are uploaded
- [x] Use `mislav/bump-homebrew-formula-action` or a custom script that:
  - Downloads SHA256SUMS from the release
  - Updates version, URL, and sha256 in the formula
  - Commits and pushes to the tap repo
- [x] Requires a PAT or fine-grained token with push access to the tap repo

### Test
- [x] `brew tap ractive/tap`
- [x] `brew install ractive/tap/hyalo`
- [x] `hyalo --help` works from Homebrew-installed binary
- [x] Test on both ARM64 and Intel Mac if possible

### Documentation
- [x] Add installation instructions to README.md (Homebrew, cargo install, manual download)

## Acceptance Criteria

- [x] `brew install ractive/tap/hyalo` works on macOS ARM64
- [x] Formula auto-updates on each new GitHub Release
- [x] README has installation instructions for all methods
