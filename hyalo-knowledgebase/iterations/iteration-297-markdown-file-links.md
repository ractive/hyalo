---
type: iteration
title: Iteration 297 — Markdown file link resolution
date: 2026-09-18
status: in-progress
tags:
  - iteration
  - links
branch: iter-297/markdown-file-links
---
# Markdown file link resolution

## Problem

A real 249-note collection reported 49 existing linked images as missing because
Markdown paths such as `images/photo.png` were checked only beneath the source
note's folder. The files existed at the scanned directory's root. Wikilinks to
those files already resolved. A separate parser defect truncated a destination
such as `images/photo%20(1).png` at its first closing parenthesis.

## Behavior

Markdown file links try the source folder first, then the scanned directory's
root. This applies equally to Markdown notes and other linked files, with no
folder-name convention. Explicit `.` or `..` path components retain relative
semantics, and leading `/` paths retain the site-prefix policy. Existing
source-relative precedence is preserved; ambiguous candidates are not guessed.

[Obsidian documents](https://obsidian.md/help/settings) relative and absolute
vault path formats for links generally. Its settings documentation does not
specify collision precedence, so this change preserves Hyalo's existing policy.

Balanced parentheses are preserved in Markdown destinations. Link extraction,
byte spans for rewriting, lint, and indexed reads share the corrected behavior.
Titles after filenames containing spaces remain separate from the destination.
Snapshot format version 3 invalidates cached destinations from the old parser;
older indexes fall back to disk until rebuilt. Single and batch moves resolve
targets before rebasing links and preserve directory-reference spelling,
including percent-encoded directory names.
Related work: [[iteration-261-link-resolution-obsidian-compat]] and
[[iteration-292-query-resolution-and-index-coherence]].

## Tasks

- [x] Reproduce all 49 false positives without changing the original collection.
- [x] Generalize root fallback across file types and resolution consumers.
- [x] Preserve explicit relative paths and source-relative precedence.
- [x] Parse balanced destination parentheses without truncating links or spans.
- [x] Add unit and CLI regressions for disk and snapshot reads, including missing targets.
- [x] Run formatting, strict workspace Clippy, and workspace tests.
- [x] Run release dogfood and the supported xtask quality gates.
- [ ] Complete independent local and Copilot reviews and reconcile findings.

## Validation

The local build resolves all 49 previously misreported image links in the source
collection. One unrelated unresolved wikilink remains reported; no rule was
disabled and no source note or configuration was modified. Synthetic fixtures
cover notes, images, directory links, explicit relative paths, missing files,
parentheses, and source-relative precedence with and without a snapshot.

The legacy `check-dead-primitives` and `check-todo-annotations` xtask commands
identify themselves as unsupported placeholders, not quality gates.
