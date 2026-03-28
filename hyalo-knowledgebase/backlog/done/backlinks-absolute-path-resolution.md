---
date: 2026-03-25
origin: dogfooding v0.3.1 against vscode-docs/docs (2,772 links, all absolute)
priority: medium
status: completed
tags:
- backlog
- backlinks
- links
- ux
title: Backlinks don't resolve site-absolute links (/docs/...)
type: backlog
---

# Backlinks don't resolve site-absolute links

## Problem

Links using site-absolute paths like `[text](/docs/configure/settings.md)` produce `"path": null` in hyalo's link output. The backlinks index is completely empty for the entire vscode-docs corpus (339 files, 2,772 links) because every link uses this convention.

Hyalo only resolves relative paths and `[[wikilinks]]`, not absolute paths. This is technically correct (hyalo is vault-relative), but limits usefulness on non-Obsidian documentation repos (Hugo, Docusaurus, VitePress, etc.).

## Proposal

When resolving a link that starts with `/`, automatically strip the `/<dir>/` prefix (derived from the existing `dir` config) before resolving. No new config option or CLI flag needed — `link-base` is always the same as `dir`.

For example, with `dir = "docs"` in `.hyalo.toml`, `/docs/configure/settings.md` → strip `/<dir>/` → `configure/settings.md` → resolves correctly against the vault root. When `dir` is `.` (repo root), just strip the leading `/`.

## Acceptance criteria

- [x] Absolute links starting with `/<dir>/` are resolved by stripping the prefix
- [x] Backlinks work on repos using site-absolute link conventions
- [x] E2e test with absolute-path links
