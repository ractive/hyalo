---
title: "Iteration 16: Robustness — Malformed Files and Path Edge Cases"
type: iteration
date: 2026-03-22
tags:
  - iteration
  - robustness
  - error-handling
  - path-resolution
status: completed
branch: iter-16/robustness
---

# Iteration 16: Robustness — Malformed Files and Path Edge Cases

Fix two bugs discovered while benchmarking against obsidian-hub (6,540 files):

1. **Malformed YAML aborts multi-file commands** — `properties`, `tags`, `set`, `remove`, and `append` hard-fail (exit 2) when any file has unparseable frontmatter, instead of skipping it. (`find` and `summary` already handle this via the scanner's `unwrap_or_default()`.)

2. **Double-dot in filenames rejected as path traversal** — `resolve_file` and `resolve_target` use `normalized.contains("..")` which false-positives on filenames like `etc..md`. The check should test for `..` as a path *component*, not as a substring.

## Tasks

### Bug 1: Gracefully handle malformed YAML in multi-file commands

- [x] Change `read_frontmatter` callers in `properties`, `tags`, `set`, `remove`, `append` to skip files with parse errors (emit warning to stderr, continue scanning)
- [x] Warning format: `warning: skipping <relative_path>: <cause>` (human-readable on stderr)
- [x] Unit test: `properties` with broken YAML returns results for valid files
- [x] Unit test: `tags` with broken YAML still aggregates tags from valid files
- [x] E2e test: `properties` succeeds with one broken file, warning on stderr
- [x] E2e test: `set` with `--glob` skips broken file, modifies valid files
- [x] Update existing `error_invalid_yaml` e2e test to match new behavior

### Bug 2: Fix path traversal check for double-dot filenames

- [x] Add `has_parent_traversal()` helper using `Path::components()` / `Component::ParentDir`
- [x] Fix `resolve_file` in `discovery.rs` — use component check instead of substring
- [x] Fix `resolve_target` in `discovery.rs` — same fix
- [x] Unit test: `resolve_file` succeeds for `"notes/etc..md"`
- [x] Unit test: `resolve_file` still rejects `"../secret.md"`, `"sub/../../etc/passwd.md"`
- [x] Unit test: `resolve_target` accepts `"etc..md"`, rejects `"../secret.md"`
- [x] E2e test: `find --file` works with a file containing `..` in its name

### Quality gates

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
- [ ] Build release and dogfood

## Acceptance criteria

- Multi-file commands never abort due to a single file's bad YAML
- `--file` works with filenames containing `..` (e.g., `etc..md`)
- All edge cases covered by tests
- Existing path traversal protection still works

## Notes

- obsidian-hub broken files: `01 - Community/Obsidian Roundup/2023-01-21 More LLM Integrations & Sample Notes for Cooking, Workouts, etc..md` and `2023-03-18 Plugins for writers & 7 new LLM-based additions..md` — both have `published:` datetime values that serde_yaml_ng rejects
- The double-dot bug also affects these same files (the `..` before `.md`)
- Both bugs are independently blocking: fixing only one still leaves the file inaccessible
- `find` and `summary` were already resilient — the scanner uses `unwrap_or_default()` at `scanner.rs:388`
