---
title: Repeatable --glob flag for combining include and exclude patterns
type: backlog
date: 2026-03-26
origin: dogfooding v0.4.0 (ISSUE-7) and v0.4.1 confirmed
priority: medium
status: completed
tags:
  - cli
  - filtering
---

`--glob` cannot be passed multiple times. Users want `--glob 'rest/**' --glob '!rest/**/index.md'` to include + exclude in one call. Currently errors with "cannot be used multiple times".

Workaround: brace expansion `--glob '{rest,graphql}/**'` works for simple cases but can't combine positive and negative patterns.

The `globset` crate already supports `GlobSet` for matching multiple patterns in a single pass. The main changes are:
- Change clap arg from `Option<String>` to `Vec<String>`
- Update `match_glob` in `discovery.rs` to accept `&[String]` and build a `GlobSet`
- Separate positive and negative patterns, apply both filters
