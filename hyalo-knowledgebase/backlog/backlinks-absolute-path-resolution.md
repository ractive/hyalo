---
title: "Backlinks don't resolve site-absolute links (/docs/...)"
type: backlog
date: 2026-03-25
origin: dogfooding v0.3.1 against vscode-docs/docs (2,772 links, all absolute)
priority: medium
status: planned
tags: [backlog, backlinks, links, ux]
---

# Backlinks don't resolve site-absolute links

## Problem

Links using site-absolute paths like `[text](/docs/configure/settings.md)` produce `"path": null` in hyalo's link output. The backlinks index is completely empty for the entire vscode-docs corpus (339 files, 2,772 links) because every link uses this convention.

Hyalo only resolves relative paths and `[[wikilinks]]`, not absolute paths. This is technically correct (hyalo is vault-relative), but limits usefulness on non-Obsidian documentation repos (Hugo, Docusaurus, VitePress, etc.).

## Proposal

Add a `link-base` config option (in `.hyalo.toml` and as `--link-base` flag) that strips a prefix before resolving. Example:

```toml
link-base = "/docs/"
```

This would make `/docs/configure/settings.md` resolve to `configure/settings.md` relative to the vault root.

## Acceptance criteria

- [ ] `--link-base /prefix/` strips the prefix from absolute links before resolution
- [ ] Backlinks work on repos using site-absolute link conventions
- [ ] Config option in `.hyalo.toml` avoids repeating the flag
- [ ] E2e test with absolute-path links and link-base config
