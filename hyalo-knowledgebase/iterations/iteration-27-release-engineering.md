---
branch: iter-27/release-engineering
date: 2026-03-23
status: planned
tags:
- iteration
- ci
- release
- distribution
title: 'Iteration 27: Release Engineering'
type: iteration
---

# Iteration 27: Release Engineering

## Goal

Bring CI/CD and release pipeline to the gold standard set by ripgrep/bat/fd. Add missing targets, improve build speed, add checksums, expand CI coverage.

## Tasks

### CI improvements
- [ ] Expand CI test job to a 3-OS matrix (ubuntu-latest, macos-latest, windows-latest)
- [ ] Pin all GitHub Actions to commit SHAs (with version comments)

### Release matrix expansion
- [ ] Add `x86_64-apple-darwin` (Intel Mac) target
- [ ] Add `x86_64-unknown-linux-musl` target (static binary)
- [ ] Add `aarch64-unknown-linux-musl` target (static binary)

### Build speed
- [ ] Replace `cargo install cross@0.2.5` with `taiki-e/install-action` for pre-built binary download

### Release artifacts
- [ ] Generate `SHA256SUMS` file and upload to GitHub Release
- [ ] Add version sync check: validate git tag matches `Cargo.toml` version

### Release notes
- [ ] Add `git-cliff` configuration for automated changelog generation
- [ ] Or: enable GitHub auto-generated release notes as a simpler alternative

### Quality gates
- [ ] Verify release workflow runs correctly with a test tag (dry-run or pre-release)
- [ ] Verify all 6+ target binaries are present in release artifacts

## Acceptance Criteria

- [ ] CI runs tests on Linux, macOS, and Windows
- [ ] Release produces binaries for: x86_64-linux-gnu, x86_64-linux-musl, aarch64-linux-gnu, aarch64-linux-musl, aarch64-apple-darwin, x86_64-apple-darwin, x86_64-windows-msvc
- [ ] SHA256SUMS file is generated and uploaded
- [ ] All actions pinned to SHAs
- [ ] Tag/version mismatch is caught before build
