---
title: "Iteration 325: move to serde-saphyr 2.1.1"
type: iteration
date: 2026-11-06
tags: [iteration, dependencies, yaml, release-blocker]
status: in-progress
branch: iter-325/serde-saphyr-2-1-1
priority: 1
related: ["[[backlog/done/serde-saphyr-2-1-0-is-yanked]]", "[[decision-log]]"]
---

# Iteration 325: move to serde-saphyr 2.1.1

2.1.0 was yanked upstream on 2026-11-05, so a fresh resolve fails and `cargo install
hyalo-cli` without `--locked` breaks. 2.1.1 is on crates.io and not yanked. See
[[backlog/done/serde-saphyr-2-1-0-is-yanked]], DEC-350, DEC-364 and DEC-375.

## Tasks

- [x] Build a baseline release binary from main (2.1.0 lock)
- [x] `cargo update -p serde-saphyr --precise 2.1.1` in the workspace and `fuzz/`, requirement raised to `2.1.1`, no other lock entry moved
- [x] Edge-case differential: 42 frontmatter blocks, 252 runs (`find --fields properties`/`file` as JSON and text, `lint`), 0 differences
- [x] Corpus differential: this knowledgebase, GitHub Docs, MDN, Obsidian Hub, kepano-obsidian, `find --fields file`, `summary`, `find --fields properties`, 15 comparisons, 0 differences
- [x] Write-path differential: `set`, `append`, `remove` on copies of the 42 blocks, 336 runs, command output and resulting file bytes, 0 differences
- [x] Probe the `Option` wrapper in `parse_yaml_map`: kept, 2.1.1 reads an empty or comments-only stream as `{}`, but `~`, `null` and `!Tag null` still fail without it
- [x] Duplicate keys still error under `DuplicateKeyPolicy::Error` (top level and nested); merge keys behave as on the baseline
- [x] `yanked = "deny"` in `deny.toml`; `cargo deny check` passes, and fails on the 2.1.0 lock
- [ ] Gates green: fmt, clippy, tests, xtask gates, npm gate, case-sensitive e2e, `hyalo lint --strict`
- [x] No snapshot format bump: parser output is identical

## Outcome

No decision beyond the bump. The snapshot format is unchanged, because DEC-375's condition
(a parser change that makes a reused entry wrong) did not occur.
