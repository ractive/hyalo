---
title: "Dogfooding v0.5.0 — consolidated report (2026-03-28, round 2)"
type: research
date: 2026-03-28
tags:
  - dogfooding
  - testing
  - v0.5.0
status: completed
---

# Dogfooding v0.5.0 — Round 2

Tested on `../docs/content` (~3520 md files, Hugo/GitHub docs) and `../vscode-docs/docs` (~339 md files, VS Code docs).

## Overall verdict

**Very stable.** No crashes, no panics, no hangs across hundreds of invocations on two real-world corpora. Performance is excellent. Recent features (iter-58 through iter-63) are all working.

## Bugs found

### BUG-1 (MEDIUM): Unrecognized operator in `--property` silently returns empty
`hyalo find --property 'title!!!broken'` returns `[]` with exit 0. Should error on unrecognized operator instead of silently matching nothing. User typos in filter operators will give wrong results with no feedback.

### BUG-2 (LOW): `--tag ''` (empty tag) silently returns empty instead of erroring
`--property ''` correctly errors, but `--tag ''` returns `[]` with exit 0. Inconsistent validation.

### BUG-3 (LOW): `--sort date` with no `date` property gives no feedback
Files are returned in default order with no warning that the sort key is missing from all files. Same for `--sort property:nonexistent`.

### BUG-4 (LOW): Date property sort is lexicographic, not date-aware
`--sort property:DateApproved` sorts "02/04/2026" before "1/18/2023" because it compares strings, not dates. Dates in M/D/YYYY format sort incorrectly.

### BUG-5 (LOW): `--limit 0` shows raw u64 range in error
Error: `0 is not in 1..18446744073709551615`. Should say "limit must be at least 1".

### BUG-6 (COSMETIC): YAML `null` values typed as "text" in properties summary
`Order: null` (or YAML tilde `~`) shows as type "text" in `properties`. Should be "null" or excluded.

### BUG-7 (COSMETIC): `hyalo versio` gets no typo suggestion
`hyalo summry` → suggests `summary`. But `hyalo versio` → no suggestion. Clap's Levenshtein threshold issue. Also `version` is not a subcommand (only `--version`), which could confuse users.

## UX issues

### UX-1 (HIGH): No way to filter by derived/display title
`--property title~=extension` only searches frontmatter, but `--fields title` shows the H1-derived title. Many docs have no `title` frontmatter, so regex search on "title" misses them. Need a virtual `title` filter or `--title` flag.

### UX-2 (MEDIUM): No reverse sort
No `--reverse`, `:desc`, or `-` prefix for sort direction. Must always sort ascending.

### UX-3 (MEDIUM): `links fix` false positives on template/external paths
`/insiders` → matched to `csharp/introvideos-csharp.md` (0.82 confidence). `/api/references/vscode-api.md` → matched to `setup/vscode-web.md` (0.88). Fuzzy matcher doesn't distinguish external URL paths from vault paths. Need `--ignore-pattern` or path-type detection.

### UX-4 (MEDIUM): No read-only `links check` subcommand
`links` only offers `fix`. Users who just want to audit broken links must use `links fix --dry-run`. A simpler `links check` would be more intuitive.

### UX-5 (LOW): Case-inconsistency in property names not detected
`Keywords` (65 files) vs `keywords` (1 file) — treated as separate properties with no warning. Unlike date-outlier warnings which already exist.

### UX-6 (LOW): `--sort date` semantics are ambiguous
When no `date` frontmatter property exists, unclear whether it sorts by modified time or a missing property.

### UX-7 (LOW): Summary output extremely verbose for large vaults
3520 files produces a 1.7MB JSON with full `by_directory` breakdown. Consider `--compact` or opt-in directory breakdown.

## What works great

- **Mutation filter guard (iter-63)**: All operators (`!=`, `>=`, `<=`, `~=`, `>`, `<`) correctly rejected with helpful error + hint about `--where-property`. Valid mutations pass through properly.
- **Sort (iter-59)**: `--sort title`, `--sort property:KEY`, `--sort modified`, `--sort backlinks_count`, `--sort links_count` — all work.
- **Find UX (iter-58)**: `--fields all/title`, `--quiet`, `--hints` — all work.
- **CLI polish (iter-60)**: Typo suggestions, bare `tags`/`properties` defaulting to summary, help text is clean.
- **Link health (iter-61)**: Broken link detection works (1651 files in docs/content, 246 in vscode-docs). Fuzzy matching works for most cases.
- **Body search**: Fast, case-insensitive, works with special chars (`${{ secrets.GITHUB_TOKEN }}`), regex mode available.
- **Index**: 10x speedup on queries, consistent results vs disk scan.
- **AND logic**: Multiple `--property` and `--tag` flags correctly AND together.
- **Error handling**: Invalid regex, empty property, unsupported format, nonexistent dir, mutually exclusive flags — all produce clear errors.

## Performance

| Command | docs/content (3520) | vscode-docs (339) |
|---------|-------------------|------------------|
| summary | 0.37s | 0.08s |
| find --fields title (all) | 0.24s | 0.05s |
| body search ("the"/all) | 0.32s | 0.06s |
| create-index | 0.32s | — |
| find from index | 0.03s | 0.01s |

## Feature requests

1. CSV output format (`--format csv`)
2. `links check` subcommand (read-only broken link listing)
3. `--ignore-pattern` for link checking (skip Hugo/template syntax)
4. Reverse sort (`--reverse` or `:desc` suffix)
5. Virtual `title` filter (search H1-derived title, not just frontmatter)
6. `--compact` mode for summary on large vaults
7. Case-insensitive regex by default in `--property K~=pattern`
8. Property name case-inconsistency warnings
