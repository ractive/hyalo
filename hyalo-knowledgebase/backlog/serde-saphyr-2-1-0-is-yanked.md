---
type: backlog
title: "serde-saphyr 2.1.0 is yanked"
date: 2026-11-05
status: planned
priority: high
origin: "dependency sweep PR, 2026-11-05"
---

## Problem

Upstream yanked serde-saphyr 2.1.0 on 2026-11-05 for an empty-document regression
(upstream #264/#265), which hyalo already works around with the `Option` wrapper in
`parse_yaml_map`. The fix, 2.1.1, is tagged on GitHub but not yet on crates.io; the index
ends at the yanked 2.1.0. Downgrading to 2.0.0 is not an option: it fails the
`iteration310_yaml_parser` and `index_incremental` e2e tests (`nuLL` and `tRUE` spellings).

This is a **release blocker**. A fresh resolve of `serde-saphyr = "^2.1.0"` fails, so
`cargo install hyalo-cli` without `--locked` fails until 2.1.1 reaches crates.io. The
workspace builds today only because `Cargo.lock` and `fuzz/Cargo.lock` still pin 2.1.0.

## Plan

- [ ] Wait for 2.1.1 with `yanked=false` on the crates.io index
- [ ] Move to 2.1.1 and read its changes
- [ ] Re-run the differentials against a release binary built from the 2.1.0 lock, and stop and decide if anything differs:
  - edge cases: a set of small frontmatter blocks (empty, comment-only, `~`, `null`, wrong-case `tRUE`/`nuLL`/`.nAn`, numbers beside Unicode spaces, tagged scalars, a BOM, `|+`/`>+` blocks, tabs after `?`), compared as `find --fields properties` JSON and as the `cause` text of the parse error
  - the five corpora (this knowledgebase, GitHub Docs, MDN, Obsidian Hub, kepano-obsidian): `find --fields file`, `summary` and `find --fields properties` compared byte for byte
  - the write path: `set`, `append` and `remove` on a copy of those frontmatter blocks, compared as the resulting file bytes
- [ ] Once the lock is off the yanked version, add `yanked = "deny"` under `[advisories]` in `deny.toml`. `cargo deny` only warns on yanked crates today; adding it now would fail CI.
