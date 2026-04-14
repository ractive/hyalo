---
title: "Dogfood v0.11.0 — lint, types, hints"
type: docs
date: 2026-04-14
status: active
tags: [dogfooding, lint, types, schema, hints]
---

# Dogfood v0.11.0 — lint, types, hints

Post-merge review of iterations 102a/b/c + 107 (surface sync).

## Bugs

1. **`lint` JSON `total` counts files, not violations** — `"total": 228` but only 13 violations. Misleading for programmatic consumers. Should separate `total_files` and `total_violations`.

2. **`summary` hints don't mention `lint` when schema has warnings** — Summary shows `schema: {files_with_issues: 13, warnings: 13}` but hints omit `hyalo lint`. Should hint lint when `files_with_issues > 0`. (Was an AC of iter-107 that wasn't delivered.)

## UX Issues

3. **`types show --format text` is unreadable** — Nested properties render flat: `properties: branch: pattern: ^iter-\d+[a-z]*/ type: string date: type: date`. No indentation or grouping. JSON is fine; text needs proper structure.

4. **`types list --format text` same problem** — Required fields one-per-line without separators. No visual grouping between types.

5. **`lint --fix --dry-run` hints miss "apply fixes"** — Normal `lint` correctly hints `--fix --dry-run` and `--fix`. But `--fix --dry-run` output only hints `types list` — should also hint `hyalo lint --fix` to apply the previewed fixes.

## Bug: `--dir` doesn't change config lookup

6. **`--dir` flag changes vault scan path but uses CWD's `.hyalo.toml` schema** — Running `hyalo lint --dir /other/repo` from hyalo's directory applies hyalo's schema (requiring `title`, `type`, etc.) to the foreign repo, causing thousands of false violations. Running the same command from the target repo's directory (`cd /other/repo && hyalo lint`) correctly finds no schema and reports no issues. Root cause: config file lookup uses CWD, not the `--dir` target.

Tested against:
- **MDN** (14,245 files): from CWD → 14,271 false errors; from its dir → 0 issues. Lint in 729ms.
- **GitHub Docs** (3,521 files): from CWD → 10,928 false errors; from its dir → 1 real error (unclosed frontmatter). Lint in 146ms.
- **VS Code Docs** (339 files): from CWD → 1,520 false errors; from its dir → 0 issues. Lint in 25ms.

## Working Well

- Hint system overall: lint and types produce contextual drill-down hints
- Enum typo correction: "planed" → "planned" (Levenshtein ≤ 2) works
- Default insertion with `$today` expansion works
- `types create`/`remove`/`set` roundtrip cleanly, TOML comments preserved
- `types set --dry-run` shows toml_changes, defaults_applied, constraint_violations
- Lint exit codes correct (1 on errors, 0 on clean)
- Performance excellent: lint 228 files in 24ms, types list in 4ms

## Perf Baselines

| Repo | Files | `lint` time |
|------|-------|-------------|
| hyalo KB | 228 | 24ms |
| VS Code Docs | 339 | 25ms |
| GitHub Docs | 3,521 | 146ms |
| MDN | 14,245 | 729ms |

Other hyalo commands (228 files): `types list` 4ms, `summary` ~18ms.
