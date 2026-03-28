---
branch: iter-53/windows-package-managers
date: 2026-03-27
status: completed
tags:
- distribution
- windows
- ci
- iteration
title: Windows Package Managers (Scoop & winget)
type: iteration
---

# Iteration 53 — Windows Package Managers (Scoop & winget)

Add Scoop and winget distribution for hyalo on Windows, plus ARM64 Windows build target.

## Tasks

- [x] Add `aarch64-pc-windows-msvc` to the release build matrix (skip tests — can't run ARM on x86 runner)
- [x] Add `scoop` job to release workflow that generates manifest and pushes to `ractive/scoop-hyalo`
- [x] Add `winget` job to release workflow using winget-releaser action
- [x] Document required secrets: `SCOOP_BUCKET_TOKEN`, `WINGET_TOKEN`
- [x] Run CI validation (fmt, clippy, test)
