---
title: Support --sort by frontmatter property value (title, date, custom)
type: backlog
date: 2026-03-28
status: completed
priority: medium
origin: dogfooding v0.4.2 on docs/content and vscode-docs/docs
---

Currently `--sort` only accepts `file`, `modified`, `backlinks_count`, `links_count`. Users naturally expect `--sort title` and `--sort date` (or any frontmatter property).

`--sort modified` is also useless on git-cloned repos where all files share the same mtime.

**Implemented behavior:**
- `--sort title` — sorts by resolved title (frontmatter `title` property, then first H1 heading)
- `--sort date` — alias for sorting by the `date` frontmatter property
- `--sort property:KEY` — generic syntax to sort by any frontmatter property value
- Files missing the sort property/title sort last
- Equal values use file path as deterministic tie-breaker

This pairs well with [[backlog/sort-reverse.md]] (if it existed) for `--reverse` support.
