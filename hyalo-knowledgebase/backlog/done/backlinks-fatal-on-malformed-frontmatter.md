---
date: 2026-03-25
origin: dogfooding v0.3.1 against docs/content (3,517 files)
priority: critical
status: completed
tags:
- backlog
- backlinks
- robustness
title: Backlinks scanner treats malformed frontmatter as fatal error
type: backlog
---

# Backlinks scanner treats malformed frontmatter as fatal error

## Problem

The `backlinks` command and `--fields backlinks` in `find` use a different code path (link graph builder) that treats malformed frontmatter as a hard error (exit code 2). All other commands (`find`, `summary`, `properties`, `tags`) gracefully skip broken files with stderr warnings.

This makes backlinks completely unusable on any vault containing even one malformed file. The docs/content corpus (3,517 files) has 4 files with bad frontmatter — enough to block all backlinks operations.

## Repro

```bash
hyalo backlinks --dir /path/to/docs/content --file actions/get-started/quickstart.md
# => Error: scanning .../admin/index.md (exit 2)

hyalo find --dir /path/to/docs/content --fields backlinks --jq 'length'
# => same fatal error
```

Meanwhile `hyalo find --dir /path/to/docs/content --jq 'length'` works fine (skips the 4 bad files with warnings).

## Proposed fix

The link graph scanner should use the same graceful skip-and-warn strategy as the normal file scanner. When a file has malformed frontmatter, emit a warning to stderr, exclude it from the link graph, and continue processing.

## Acceptance criteria

- [ ] `backlinks` command completes on vaults with malformed files (warns and skips)
- [ ] `--fields backlinks` in `find` completes on vaults with malformed files
- [ ] Exit code is 0 when broken files are skipped
- [ ] Warning messages include the file path
- [ ] E2e test: backlinks on a vault with a broken file succeeds
