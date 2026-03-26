---
title: "RAII index server with file watcher"
type: backlog
date: 2026-03-26
tags:
  - performance
  - index
  - daemon
---

# RAII Index Server with File Watcher

Long-running `hyalo index-server` process that:
- Creates and holds a `.hyalo-index` file
- Watches the vault directory for changes via `notify` crate
- Atomically updates the index on file changes (write temp → rename)
- Cleans up the index file on exit (SIGINT/SIGTERM via `ctrlc` crate)

Useful when a skill session both reads and mutates files, so subsequent queries see
updated state without a full rescan.

Depends on: [[iteration-47-snapshot-index]] (snapshot index infrastructure).

## Alternative: stdin/stdout protocol

Instead of or alongside the file-based approach, serve queries directly over pipes.
Avoids temp file management entirely — parent dies → pipe closes → child exits.
Trade-off: only usable by a single client process.
