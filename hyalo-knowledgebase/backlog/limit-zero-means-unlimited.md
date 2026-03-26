---
title: "--limit 0 should mean unlimited, not zero results"
type: backlog
date: 2026-03-26
origin: dogfooding v0.4.1
priority: low
status: planned
---

`hyalo find --limit 0` returns `[]` (empty array). Most CLI tools treat `--limit 0` as "no limit / unlimited". This is a minor footgun.

Fix: treat `--limit 0` the same as omitting `--limit` entirely. In the find command, convert `Some(0)` to `None` before passing the limit downstream.
