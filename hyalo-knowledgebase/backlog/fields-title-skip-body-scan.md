---
title: "Skip body scan for --fields title when frontmatter has title"
type: backlog
date: 2026-03-28
status: planned
priority: low
origin: Copilot review on PR #68 (iter-58)
tags:
  - performance
  - find
---

When `--fields title` is set, `needs_body()` returns `true` unconditionally, triggering the full `SectionScanner` on every file. But if the frontmatter already contains a `title` property (the common case), the body scan is unnecessary — the title is resolved from frontmatter without needing the H1 heading.

**Fix:** Parse frontmatter first, check for a string `title` property, and only fall back to body scanning for the H1 when it's absent. This avoids the `SectionScanner` overhead on files that already have a frontmatter title.

**Note:** The index path (`--index`) already resolves titles from `entry.properties` and `entry.sections` without any body scan, so this issue only affects the disk-scan path. Using `--index` is the recommended workaround for large vaults.

**Impact:** Proportional to vault size and how many files have frontmatter titles. Negligible on small vaults; potentially noticeable on large ones where most files already declare `title` in frontmatter.
