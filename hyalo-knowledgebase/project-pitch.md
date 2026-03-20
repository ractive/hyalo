---
title: "Hyalo — Project Pitch"
type: pitch
date: 2026-03-20
status: active
tags:
  - project
  - pitch
---

# Hyalo — Project Pitch

A self-contained CLI tool for exploring and managing Markdown knowledge bases.
Compatible with [Obsidian](https://obsidian.md/) markdown files — no running Obsidian instance required.

## Problem

The Obsidian CLI requires a running Obsidian application with an open vault. This makes it unusable for headless environments, CI pipelines, and AI coding agents that need to programmatically work with markdown knowledge bases.

## Goal

Build a standalone Rust CLI tool that can:

1. **Parse and manage Obsidian-compatible markdown files** — including YAML frontmatter with typed properties
2. **Provide powerful search** — query files by frontmatter properties, tags, content, and links
3. **Navigate the link graph** — find outgoing links, backlinks, orphans, and dead ends
4. **Manage structured data** — read/set/remove frontmatter properties with correct typing
5. **Work with tasks** — list, filter, and toggle markdown task checkboxes
6. **Understand document structure** — extract outlines (headings), tags, and metadata

## Non-Goals

We do **not** reimplement Obsidian application features:
- No vault management (`.obsidian/` config, plugins, themes)
- No file history or sync
- No bookmarks or daily notes
- No template engine
- No publish functionality
- No file create/read/append/delete (AI agents handle this natively)

## Key Commands (Initial Vision)

Based on the Obsidian CLI, the most valuable commands for AI agents are:

| Command | Purpose |
|---------|---------|
| `search` | Query files by content, properties, tags, paths |
| `properties` | List/read/set/remove frontmatter properties |
| `tags` | List and filter tags across files |
| `tasks` | List, filter, toggle tasks |
| `outline` | Extract heading structure |
| `links` | Show outgoing links from a file |
| `backlinks` | Show incoming links to a file |
| `unresolved` | Find broken/unresolved links |
| `orphans` | Find files with no incoming links |
| `deadends` | Find files with no outgoing links |
| `move` / `rename` | Move/rename files and update all internal links |

## Search Query Syntax (Target)

Obsidian's search syntax is the gold standard. Target compatibility:

```
# Property queries
[status:ready]
[type:story]
[priority:high]
[duration:<5]
[duration:>5]

# Boolean logic
meeting work
meeting OR work
-draft
(status:ready OR status:review)

# Operators
file:*.md
path:"backlog/"
tag:#sprint-3
content:"error handling"
task-todo:implement
task-done:review
```

## Future: Indexing

For large knowledge bases, property-based search benefits from an index. This is a later-stage optimization — start with direct file scanning, add indexing when performance demands it.

## Tech Stack

- **Rust** (2024 edition)
- **clap** for CLI parsing
- **serde** / **serde_yaml** for frontmatter
- Additional crates TBD per iteration
