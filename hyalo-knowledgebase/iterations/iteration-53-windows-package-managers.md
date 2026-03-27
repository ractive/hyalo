---
title: "Windows Package Managers (Scoop & winget)"
type: iteration
date: 2026-03-27
tags:
  - distribution
  - windows
  - ci
status: in-progress
branch: iter-53/windows-package-managers
---

# Iteration 53 — Windows Package Managers (Scoop & winget)

Add Scoop and winget distribution for hyalo on Windows, plus ARM64 Windows build target.

## Tasks

- [ ] Add `aarch64-pc-windows-msvc` to the release build matrix (skip tests — can't run ARM on x86 runner)
- [ ] Add `scoop` job to release workflow that generates manifest and pushes to `ractive/scoop-hyalo`
- [ ] Add `winget` job to release workflow using winget-releaser action
- [ ] Document required secrets: `SCOOP_BUCKET_TOKEN`, `WINGET_TOKEN`
- [ ] Run CI validation (fmt, clippy, test)
