---
branch: iter-1/frontmatter-properties
date: 2026-03-20
status: completed
tags:
  - iteration
  - frontmatter
  - properties
title: Iteration 1 — Frontmatter Parser & Property Commands
type: iteration
---

# Iteration 1 — Frontmatter Parser & Property Commands

## Goal

Parse YAML frontmatter from markdown files, infer property types, and provide CLI commands to read/list/set/remove properties. After this iteration, hyalo is a useful tool for AI agents working with structured markdown.

## CLI Interface

Target root directory is controlled by a global `--dir` option (default `.`):

```sh
# List all properties across files
hyalo properties [--path <glob>] [--format json|text]

# Read all properties of a file
hyalo properties --path path/to/file.md [--format json|text]

# Read a single property
hyalo property read --name status --path path/to/file.md

# Set a property (type inferred or explicit)
hyalo property set --name status --value "in-progress" --path path/to/file.md
hyalo property set --name priority --value 3 --type number --path path/to/file.md

# Remove a property
hyalo property remove --name status --path path/to/file.md
```

See [[decision-log]] for cross-cutting design decisions (CLI style, `--dir`, `--path`, `--format`, error output, frontmatter rewrite strategy).

## Tasks

### Crate & CLI Setup
- [x] Set up clap with subcommands: `properties`, `property read`, `property set`, `property remove`
- [x] Add `--dir` global option (default `.`)
- [x] Add `--path` option for file/glob targeting
- [x] Add `--format` option (json, text) with json as default
- [x] Add `--name`, `--value`, `--type` options for property subcommands

### Frontmatter Parser
- [x] Add `serde_yaml_ng` dependency (replaces deprecated `serde_yaml`)
- [x] Extract YAML frontmatter from between `---` delimiters
- [x] Parse into a `BTreeMap<String, Value>` (preserves key order)
- [x] Infer property type from YAML value: text, number, checkbox, date, datetime, list
- [x] Handle edge cases: no frontmatter, empty frontmatter, unclosed frontmatter (error)
- [x] Preserve non-frontmatter content when writing back
- [x] Streaming reader (`read_frontmatter`) for read-only ops — stops at closing `---`, never reads body
- [x] Line/byte budget on streaming reader to prevent buffering entire file on missing closer

### File Discovery
- [x] Walk directory tree recursively, collecting `*.md` files
- [x] Support `--path` as exact file path or glob pattern
- [x] Respect `.gitignore` patterns (use `ignore` crate)
- [x] Skip hidden directories (`.obsidian/`, `.git/`, etc.)
- [x] Reject path traversal (`..`, absolute paths) to sandbox operations under `--dir`
- [x] Normalize path separators to forward slashes (cross-platform)

### Property Commands
- [x] `properties` — list all properties of a file (or aggregate across files)
- [x] `property read` — read a single named property, output its value
- [x] `property set` — set a property value with type inference or explicit `--type`
- [x] `property remove` — remove a property from frontmatter
- [x] Create frontmatter block if file has none (for `property set`)
- [x] `CommandOutcome` enum for clean success/user-error separation (no magic exit codes)

### Output Formatting
- [x] JSON output (default): structured, machine-readable
- [x] Text output: human-readable key: value pairs

### Unit Tests (in-module `#[cfg(test)]`)
- [x] Frontmatter extraction: valid, missing, empty, malformed/unclosed
- [x] Type inference: text, number, bool, date, datetime, list
- [x] Property set: add new, overwrite existing, create frontmatter if absent
- [x] Property remove: existing key, missing key (no-op), last key (empty frontmatter)
- [x] Roundtrip: set then read returns same value; body content preserved
- [x] Streaming reader parity with full parse
- [x] Path traversal rejection

### E2E Tests (`tests/` directory, `assert_cmd` + `tempfile`)
- [x] `hyalo properties --path file.md` — reads all properties as JSON
- [x] `hyalo property read --name <key> --path file.md` — outputs value
- [x] `hyalo property read` — missing property returns exit code 1
- [x] `hyalo property set --name <key> --value <val> --path file.md` — mutates file correctly
- [x] `hyalo property set` on file without frontmatter — creates frontmatter
- [x] `hyalo property remove --name <key> --path file.md` — removes property, body intact
- [x] `hyalo properties` (no --path) — aggregates across all .md files in --dir
- [x] `--format json` / `--format text` output validation
- [x] Error cases: nonexistent file, nonexistent dir, invalid YAML, missing .md extension
- [ ] Smoke test: run against `hyalo-knowledgebase/` files

### Quality Gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`

## Acceptance Criteria

1. [x] `hyalo properties --path file.md --format json` outputs all frontmatter properties with inferred types
2. [x] `hyalo property read --name status --path file.md` outputs the value of a single property
3. [x] `hyalo property set --name status --value done --path file.md` updates the property in-place without disturbing other content
4. [x] `hyalo property remove --name status --path file.md` removes the property
5. [x] `hyalo properties --format json` aggregates properties across all `.md` files in the root
6. [x] All commands exit with appropriate codes (0 success, 1 not found, 2 error)
7. [ ] Dogfooding: hyalo can read its own knowledgebase files' frontmatter correctly

## Dependencies (Crates)

- `clap` — CLI parsing
- `serde` + `serde_yaml_ng` — YAML frontmatter (replaced deprecated `serde_yaml`)
- `serde_json` — JSON output
- `ignore` — gitignore-aware file walking
- `anyhow` — error handling
- `globset` — path matching

### Dev Dependencies
- `assert_cmd` — run binary in tests, assert on stdout/stderr/exit code
- `predicates` — fluent assertions for assert_cmd
- `tempfile` — per-test temp directories (no shared fixtures)

## Learnings & Review Findings

### Code Review (2026-03-20)
- **`serde_yaml` is deprecated.** Migrated to `serde_yaml_ng` 0.10 — actively maintained community fork, drop-in API replacement. Avoid `serde_yml` (RUSTSEC-2025-0068, unsound).
- **Unclosed frontmatter is dangerous for mutations.** If `Document::parse` silently treats unclosed `---` as "no frontmatter", then `property set` writes a new block on top, corrupting the file. Now errors on unclosed frontmatter.
- **Streaming vs full parse must agree on semantics.** `read_frontmatter` (streaming, read-only) and `Document::parse` (full, for mutations) had different behavior on missing closers. Now both treat it as an error condition.
- **Path traversal was unguarded.** `resolve_file` allowed `../` and absolute paths to escape `--dir`. Now rejects them.
- **Windows path separators break glob matching.** `to_string_lossy()` uses `\` on Windows. Must normalize to `/` for consistent glob matching and output.
- **`NaN`/`inf` parse as valid f64.** Must explicitly check `is_finite()` in both inference and forced-type paths.
- **`CommandOutcome` is cleaner than `(String, i32)`.** Enum makes success vs user-error intent explicit, eliminates magic numbers.

## Notes

- **Formatting preservation: not needed.** serde_yaml_ng rewrites the full frontmatter on set/remove. Obsidian itself does the same. Keeps iteration 1 simple.
- YAML frontmatter in Obsidian is always flat (no nested objects) — we can rely on this
- Internal links in property values (`"[[Note]]"`) are just strings for now — link parsing comes in iteration 2
- The `tags` property is a list type but has special semantics — tag aggregation comes in iteration 3
