---
title: "Iteration 1 — Frontmatter Parser & Property Commands"
type: iteration
date: 2026-03-20
status: planned
branch: iter-1/frontmatter-properties
tags:
  - iteration
  - frontmatter
  - properties
---

# Iteration 1 — Frontmatter Parser & Property Commands

## Goal

Parse YAML frontmatter from markdown files, infer property types, and provide CLI commands to read/list/set/remove properties. After this iteration, hyalo is a useful tool for AI agents working with structured markdown.

## CLI Interface

Target root directory is passed as a positional arg or defaults to `.`:

```sh
# List all properties across files
hyalo properties [--path <glob>] [--format json|yaml|text]

# Read all properties of a file
hyalo properties --path path/to/file.md [--format json|yaml|text]

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
- [ ] Set up clap with subcommands: `properties`, `property read`, `property set`, `property remove`
- [ ] Add `--dir` global option (default `.`)
- [ ] Add `--path` option for file/glob targeting
- [ ] Add `--format` option (json, yaml, text) with json as default
- [ ] Add `--name`, `--value`, `--type` options for property subcommands

### Frontmatter Parser
- [ ] Add `serde_yaml` dependency
- [ ] Extract YAML frontmatter from between `---` delimiters
- [ ] Parse into a `BTreeMap<String, serde_yaml::Value>` (preserves key order)
- [ ] Infer property type from YAML value: text, number, checkbox, date, datetime, list
- [ ] Handle edge cases: no frontmatter, empty frontmatter, JSON frontmatter
- [ ] Preserve non-frontmatter content when writing back

### File Discovery
- [ ] Walk directory tree recursively, collecting `*.md` files
- [ ] Support `--path` as exact file path or glob pattern
- [ ] Respect `.gitignore` patterns (use `ignore` crate)
- [ ] Skip hidden directories (`.obsidian/`, `.git/`, etc.)

### Property Commands
- [ ] `properties` — list all properties of a file (or aggregate across files)
- [ ] `property read` — read a single named property, output its value
- [ ] `property set` — set a property value with type inference or explicit `--type`
- [ ] `property remove` — remove a property from frontmatter
- [ ] Preserve existing YAML formatting: don't rewrite untouched properties
- [ ] Create frontmatter block if file has none (for `property set`)

### Output Formatting
- [ ] JSON output (default): structured, machine-readable
- [ ] YAML output: frontmatter-style
- [ ] Text output: human-readable key: value pairs

### Unit Tests (in-module `#[cfg(test)]`)
- [ ] Frontmatter extraction: valid, missing, empty, malformed, JSON-in-YAML
- [ ] Type inference: text, number, bool, date, datetime, list
- [ ] Property set: add new, overwrite existing, create frontmatter if absent
- [ ] Property remove: existing key, missing key (no-op), last key (empty frontmatter)
- [ ] Roundtrip: set then read returns same value; body content preserved

### E2E Tests (`tests/` directory, `assert_cmd` + `tempfile`)
- [ ] `hyalo properties --path file.md` — reads all properties as JSON
- [ ] `hyalo property read --name <key> --path file.md` — outputs value
- [ ] `hyalo property read` — missing property returns exit code 1
- [ ] `hyalo property set --name <key> --value <val> --path file.md` — mutates file correctly
- [ ] `hyalo property set` on file without frontmatter — creates frontmatter
- [ ] `hyalo property remove --name <key> --path file.md` — removes property, body intact
- [ ] `hyalo properties` (no --path) — aggregates across all .md files in --dir
- [ ] `--format json` / `--format yaml` / `--format text` output validation
- [ ] Error cases: nonexistent file, nonexistent dir, invalid YAML
- [ ] Smoke test: run against `hyalo-knowledgebase/` files

### Quality Gates
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`

## Acceptance Criteria

1. `hyalo properties --path file.md --format json` outputs all frontmatter properties with inferred types
2. `hyalo property read --name status --path file.md` outputs the value of a single property
3. `hyalo property set --name status --value done --path file.md` updates the property in-place without disturbing other content
4. `hyalo property remove --name status --path file.md` removes the property
5. `hyalo properties --format json` aggregates properties across all `.md` files in the root
6. All commands exit with appropriate codes (0 success, 1 not found, 2 error)
7. Dogfooding: hyalo can read its own knowledgebase files' frontmatter correctly

## Dependencies (Crates)

- `clap` (already present) — CLI parsing
- `serde` + `serde_yaml` — YAML frontmatter
- `serde_json` (already present) — JSON output
- `ignore` — gitignore-aware file walking
- `anyhow` — error handling
- `glob` or `globset` — path matching

### Dev Dependencies
- `assert_cmd` — run binary in tests, assert on stdout/stderr/exit code
- `predicates` — fluent assertions for assert_cmd
- `tempfile` — per-test temp directories (no shared fixtures)

## Notes

- **Formatting preservation: not needed.** serde_yaml rewrites the full frontmatter on set/remove. Obsidian itself does the same. Keeps iteration 1 simple.
- YAML frontmatter in Obsidian is always flat (no nested objects) — we can rely on this
- Internal links in property values (`"[[Note]]"`) are just strings for now — link parsing comes in iteration 2
- The `tags` property is a list type but has special semantics — tag aggregation comes in iteration 3
