---
date: 2026-03-21
origin: iteration-02 known limitation, DEC-015
priority: high
status: done
tags:
- backlog
- scanner
- links
title: Scanner should skip %%comment%% blocks
type: backlog
---

# Scanner should skip %%comment%% blocks

## Problem

Obsidian `%%comment%%` blocks are not tracked by the scanner. Links, tags, and tasks inside comments are incorrectly extracted. For example, a commented-out `[[draft-note]]` wikilink shows up in `links` output and in outline sections.

## Proposal

Add comment block state tracking to the scanner, similar to fenced code block tracking. When inside `%%...%%`, skip all link/task/heading extraction.

Multi-line comments: `%%` on its own line opens/closes.
Inline comments: `%%text%%` on a single line.

## Scope

Small — the scanner already tracks fenced code block state with the same open/close pattern. This is a direct analogue.

## References

- [[decision-log#DEC-015]]: documented as known limitation, explicitly noted as "straightforward to add"
- [[iteration-02-links]]: listed under Known Limitations

## My Comments
With our own "parser", this could be done easily I guess?