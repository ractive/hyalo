---
date: 2026-03-23
origin: dogfooding docs/content vault (3,521 files)
priority: critical
status: completed
tags:
- backlog
- robustness
- cli
title: 'Graceful parse error recovery: skip broken files instead of hard exit'
type: backlog
---

# Graceful parse error recovery: skip broken files instead of hard exit

## Problem

A single file with malformed frontmatter (unclosed `---`, or frontmatter exceeding the 100-line/8KB streaming budget) causes hyalo to hard-exit with code 2, aborting the entire scan. This makes hyalo unusable on real-world vaults like GitHub's docs/content (3,521 files) where even one offender kills everything.

Iter-16 added robustness for *YAML parse errors* (skip + warn), but two cases still hard-exit:

1. **Unclosed frontmatter** — file starts with `---` but never closes it (frontmatter-only index files, common in Hugo/GitHub docs)
2. **Frontmatter exceeds streaming budget** — valid frontmatter > 100 lines / 8KB (large index files with long `redirect_from` or `children` lists)

## Severity

**Critical** — this was the single biggest blocker during dogfooding. Had to copy 3,500 files to a temp dir and manually exclude offenders to proceed.

## Additionally

The error message does **not include the file path**, making it impossible to identify which file caused the failure without manual binary-search scripting.

## Proposed fix

- Treat both conditions as warnings (stderr), skip the file, continue scanning
- Include the file path in all parse error/warning messages
- Consider making the streaming budget configurable (e.g. `--max-frontmatter-lines 200`)

## Acceptance criteria

- [ ] Unclosed frontmatter emits warning to stderr, skips file, continues scan
- [ ] Frontmatter exceeding budget emits warning to stderr, skips file, continues scan
- [ ] All warning messages include the file path
- [ ] `hyalo summary --dir <3500-file-vault>` completes successfully even with broken files
- [ ] Exit code is 0 when broken files are skipped (not 2)

## My Comments
Explore how the "index" files look like that are use in the github docs repo. I read somewhere that index file start with "---" but don't have a closing "---". Should we support them?