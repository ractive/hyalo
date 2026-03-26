---
date: 2026-03-26
status: completed
tags:
- dogfooding
- v0.4.1
- external-docs
title: 'Dogfooding Report: v0.4.1 Creative & Stress Testing on External Repos'
type: research
---

# Dogfooding Report: v0.4.1 Creative & Stress Testing on External Repos

Systematic testing of hyalo 0.4.1 against GitHub Docs (3520 files) and VS Code Docs (339 files), focusing on creative usage, edge cases, error handling, pipe-friendliness, and concurrency.

## Test Corpora

- **GitHub Docs** (`docs/content`): 3520 files, 36 unique properties, 0 tags, 0 tasks. Heavy use of `redirect_from` lists (2446 files), `versions` as JSON-in-string, `children` lists.
- **VS Code Docs** (`vscode-docs/docs`): 339 files, 13 properties, 0 tags, 48 orphans. All links use `/docs/` prefix pattern.

## Performance

| Command | Corpus | Time |
|---------|--------|------|
| `summary` | GitHub Docs (3520 files) | 0.22s |
| `summary` | VS Code Docs (339 files) | 0.05s |
| `find --property 'title~=/[Aa]uthenticat/'` | GitHub Docs | 0.17s |
| `find --glob '**/*index*.md' --fields properties` | GitHub Docs | 0.07s |
| `find --fields links,backlinks` (full scan + backlink graph) | GitHub Docs | 0.50s |
| Concurrent: two `find` queries in parallel | GitHub Docs | 0.31s (wall clock, both finished) |

Performance is excellent. Full backlink graph scan of 3520 files in 0.5s is impressive. Concurrent access works without issue.

## What Works Really Well

1. **Built-in `--jq` flag** -- Extremely powerful. Enables complex one-liners like finding top directories by auth content, counting broken links, or computing property value distributions. No need for external `jq`.

2. **Content + property + section filter combinations** -- AND-composable filters are genuinely useful. `find "OAuth" --property 'title~=/[Aa]uthenticat/' --fields links` immediately finds the intersection.

3. **Pipe-friendliness** -- Warnings go to stderr, data to stdout. `--jq '.[].file' | wc -l` works perfectly. `--jq '.[].file' | xargs ...` works for scripting.

4. **Error messages** -- Clear and actionable:
   - Invalid regex: `"invalid regex in property filter"` with the offending filter quoted
   - Non-existent file: `"file not found"` with path
   - Empty property name: `"property filter name must not be empty"`
   - Flag conflicts: clap-generated usage message with conflicting flags named
   - `--jq` with `--format text`: Clear message explaining they're incompatible

5. **`--hints` mode** -- Genuinely helpful for exploration. After `summary`, it suggests `properties` and `tags` as next steps.

6. **`--fields backlinks`** in `find` -- Makes the expensive backlink scan opt-in. Smart design.

7. **Section regex matching** -- `--section '~=/Step [0-9]+/'` correctly finds numbered steps. The substring default and regex opt-in are well-designed.

8. **`read --section`** -- Extracting a specific section from a file is a killer feature for scripting. `read --file X --section 'Device flow'` instantly returns just that section.

9. **`--limit 0`** returns empty array, not an error. Consistent behavior.

10. **Broken frontmatter handling** -- The `code-security/concepts/index.md` file genuinely has no closing `---`. Hyalo warns on stderr and skips it during scanning. Good resilience.

## Bugs Found

### BUG-1: `--dir` pointing to a file silently succeeds with empty paths
When `--dir` points to a file instead of a directory (e.g., `--dir /path/to/index.md`), hyalo doesn't error. It shows summary output with `path: ""` (empty string) in recent_files and orphans. Should either error or resolve the path correctly.

### BUG-2: `read --frontmatter` succeeds on broken frontmatter files
`hyalo read --file code-security/concepts/index.md` correctly errors with "unclosed frontmatter". But `hyalo read --file code-security/concepts/index.md --frontmatter` succeeds and outputs the content wrapped in `---` delimiters (fabricating a closing `---` that doesn't exist). The `--frontmatter` flag should not suppress the broken-frontmatter error.

### BUG-3: Inequality filter `!=` returns empty on checkbox properties
`find --property 'hidden!=true' --property 'hidden'` (files where hidden exists but is not true) should work but returned empty. Need to verify this is actually a bug vs expected behavior with checkbox type coercion.

## UX Issues

### UX-1: No `--sort-by count` on `tags summary` or `properties summary`
Tags and properties summaries sort alphabetically. For large vaults (36 properties), sorting by count would be useful for finding the most/least common properties. Currently you must pipe through `--jq 'sort_by(.count) | reverse'`, which works but is less discoverable.

### UX-2: No reverse sort option for `find --sort`
`find --sort modified` sorts ascending (oldest first). There's no `--sort modified:desc` or `--reverse` flag. To get newest-first, you need `--jq 'reverse'`, which defeats streaming.

### UX-3: `--format text` on `summary` truncates orphans list at ~48 entries
The VS Code Docs summary showed all 48 orphans in text mode, but GitHub Docs (934 orphans) was truncated. Text mode should show a count and sample, not attempt to list all.

### UX-4: Non-matching glob returns error instead of empty result
`find --glob 'zzz-nonexistent/**/*.md'` returns `{"error": "no files match pattern"}` with exit code 1. This makes scripting harder -- a glob with no matches should arguably return an empty array `[]` with exit code 0, since "no results" is not an error condition.

### UX-5: `tags summary` text output says "0 unique tags" with no additional context
When a vault has no tags at all, the output is just `0 unique tags`. Could mention "No YAML `tags:` frontmatter found" or suggest checking if the vault uses a different tagging convention.

## Missing Features That Would Be Useful

### FEAT-1: `--content` flag as alias for positional PATTERN
The positional pattern for body search is easy to miss. Having `find --content "OAuth" --property 'title~=auth'` would be more discoverable and self-documenting than `find "OAuth" --property 'title~=auth'`.

### FEAT-2: Link resolution with configurable site prefix
VS Code Docs has 2549 unresolved links because they all start with `/docs/` (the vault directory name). A config option like `site_prefix = "/docs"` in `.hyalo.toml` would let hyalo strip this prefix during resolution, making `backlinks` work correctly on these repos.
(Note: This was identified in the v0.4.0 dogfood report too -- still unresolved.)

### FEAT-3: `find --has-broken-links` filter
Finding files with unresolved links requires a complex jq filter: `--jq '[.[] | select(.links | map(select(.path == null)) | length > 0)]'`. A dedicated `--has-broken-links` flag would be much more ergonomic.

### FEAT-4: `properties summary --sort-by count`
Direct sort option on properties/tags summary commands, rather than requiring jq.

### FEAT-5: `find --sort modified:desc` or `find --sort -modified`
Reverse sort without requiring jq post-processing.

### FEAT-6: `find --property 'contentType' --values` or `properties values contentType`
Show unique values for a given property. Currently requires: `find --fields properties --jq '[.[].properties.contentType] | group_by(.) | map({v: .[0], n: length}) | sort_by(.n) | reverse'`. Useful for understanding property value distributions.

## Comparison: hyalo vs rg+find

### Task 1: "Find all tutorial pages about authentication"
- **rg+find**: `rg -l 'contentType: tutorials' docs/content | xargs rg -l -i 'authenticat'` -- Two passes, fragile YAML parsing.
- **hyalo**: `hyalo find --property 'contentType=tutorials' --property 'title~=/authenticat/i'` -- Single command, correct YAML parsing, typed comparison.
- **Winner**: hyalo, by a wide margin.

### Task 2: "What pages link to the OAuth authorization guide?"
- **rg+find**: `rg -l 'authorizing-oauth-apps' docs/content/` -- Fast but finds any string match including redirect_from, not just links.
- **hyalo**: `hyalo find --fields links --jq '[.[] | select(.links[]? | .target | test("authorizing-oauth-apps")) | .file]'` -- Only actual links, with source context.
- **Winner**: hyalo for precision, rg for speed.

### Task 3: "Find hidden deprecated pages with specific title patterns"
- **rg+find**: Requires multi-line regex or multiple passes to match frontmatter structure.
- **hyalo**: `hyalo find --property 'hidden=true' --property 'title~=/deprecated/i'` -- One command.
- **Winner**: hyalo, no contest.

### Task 4: "Extract just the 'Prerequisites' section from a tutorial"
- **rg+find**: `sed`/`awk` gymnastics to extract between headings.
- **hyalo**: `hyalo read --file path.md --section 'Prerequisites'` -- One command, handles nesting.
- **Winner**: hyalo, dramatically simpler.

## Summary

hyalo 0.4.1 is highly capable for documentation querying. The `--jq` integration turns it into a documentation-specific SQL engine. Performance on 3500+ files is excellent (sub-second for everything). The two real bugs (BUG-1, BUG-2) are minor. The biggest gap remains link resolution for repos using site-prefix URLs (FEAT-2), which affects backlinks usefulness on real-world doc sites.
