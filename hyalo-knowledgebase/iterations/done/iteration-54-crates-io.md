---
branch: iter-54/crates-io
date: 2026-03-27
status: completed
tags:
- iteration
- distribution
- crates-io
title: Iteration 54 — Publish to crates.io
type: iteration
---

# Iteration 54 — Publish to crates.io

## Goal

Add crates.io as a distribution channel so Rust developers can install hyalo via `cargo install hyalo-cli`.

## Motivation

Hyalo already ships pre-built binaries via GitHub Releases, Homebrew, Scoop, and winget. Publishing to crates.io covers the Rust developer audience who naturally discover and install tools with `cargo install`. It's zero-cost to maintain once set up.

## Tasks

- [ ] Remove `publish = false` from workspace Cargo.toml
- [ ] Add `version` field to workspace `hyalo-core` dependency for crates.io resolution
- [ ] Add crates.io metadata to `hyalo-core` (description, repository, keywords, categories)
- [ ] Add crates.io metadata to `hyalo-cli` (description, repository, readme, keywords, categories)
- [ ] Add `crates-io` job to release workflow (publish hyalo-core then hyalo-cli)
- [ ] Verify with `cargo publish --dry-run`
- [ ] Pass all quality gates (fmt, clippy, test)

## References

- [[iterations/done/iteration-53-windows-package-managers]] — prior distribution work
- [[iterations/done/iteration-31-homebrew-distribution]] — initial Homebrew setup
