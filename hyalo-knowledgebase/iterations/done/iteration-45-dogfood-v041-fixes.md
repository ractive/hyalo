---
title: Iteration 45 — Dogfood v0.4.1 Bugfixes
type: iteration
date: 2026-03-26
tags:
  - iteration
  - dogfooding
  - bug-fix
status: completed
branch: iter-45/site-prefix-fix
---

# Iteration 45 — Dogfood v0.4.1 Bugfixes

## Goal

Fix all bugs found dogfooding v0.4.1 against GitHub Docs and VS Code Docs. Six PRs merged, each addressing a specific issue.

## PRs

- PR #48: site-prefix-fix — Fix site_prefix derivation for all --dir invocation styles
- PR #49: regex-eq-tilde-alias — Add `=~` as alias for `~=` regex operator
- PR #50: exclude-self-backlinks — Exclude self-links from backlinks results
- PR #51: empty-glob-result — Return empty result instead of error for non-matching globs
- PR #52: limit-short-circuit — Short-circuit `--limit` when sort order allows early termination
- PR #53: remaining-fixes — `--dir` file validation, warning dedup, `read --frontmatter` validation

## Tasks

- [x] Fix site_prefix derivation: canonicalize path, extract file_name(), tri-state precedence (CLI > config > auto)
- [x] Add `--site-prefix` CLI flag and `.hyalo.toml` `site_prefix` key
- [x] Add `=~` as Perl-style alias for `~=` with guard against `!=~`, `>=~`, `<=~`
- [x] Exclude self-links from backlinks output (filter at CLI layer, not core)
- [x] Return empty result instead of error for non-matching globs
- [x] Short-circuit `--limit` when sort is default (file path), no backlinks, no explicit `--file`
- [x] Error when `--dir` points to a file instead of a directory
- [x] Deduplicate warnings when using `--fields links,backlinks`
- [x] Validate frontmatter in `read --frontmatter` mode (error on unclosed frontmatter)
- [x] E2e tests for all fixes (7 site-prefix tests, plus tests for each PR)
- [x] All 6 PRs reviewed (local /review-rust + Copilot), feedback addressed
- [x] All 6 PRs merged to main

## Acceptance Criteria

- [x] All 4 invocation styles for `--dir` produce correct site_prefix
- [x] `=~` works as alias for `~=` without ambiguity
- [x] Self-links excluded from backlinks but still rewritten by mv
- [x] Non-matching globs return empty result (exit 0)
- [x] `--limit` short-circuits file discovery when safe
- [x] All quality gates pass (414 tests)
