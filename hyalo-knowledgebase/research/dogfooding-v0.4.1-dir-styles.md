---
date: 2026-03-26
status: completed
tags:
- dogfooding
- v0.4.1
- external-docs
title: 'Dogfooding Report: v0.4.1 Directory Invocation Styles'
type: research
---

# Dogfooding Report: v0.4.1 Directory Invocation Styles

Tested hyalo v0.4.1 with four different ways of specifying the target directory, checking for consistency in path resolution, link resolution, backlink discovery, and site_prefix behavior.

## Test Repos

- **GitHub Docs** (`~/devel/docs/content`): 3520 files, absolute links like `/organizations/...`
- **VS Code Docs** (`~/devel/vscode-docs/docs`): 339 files, absolute links like `/docs/azure/...`

## Four Invocation Styles

| Style | CWD | --dir value |
|-------|-----|-------------|
| 1 | `~/devel/hyalo` | `../vscode-docs/docs` (relative) |
| 2 | `~/devel/vscode-docs/docs` | (none, uses CWD) |
| 3 | `/tmp` | `~/devel/vscode-docs/docs` (tilde path; expands to absolute in shell) |
| 4 | `~/devel/vscode-docs` | `docs` (subfolder name) |

## Results Summary

### All Styles Work (no crashes, no errors)
All four styles run without errors for `summary`, `find`, and `backlinks` commands.

### File Paths in Output: Consistent
All styles output file paths relative to the docs directory (e.g., `azure/overview.md`). No absolute paths leak into output.

### Warnings: Consistent
GitHub docs always produces `warning: skipping code-security/concepts/index.md: unclosed frontmatter`. VS Code docs produces no warnings. Consistent across all styles.

### Link Resolution: BUG - Inconsistent across styles

**GitHub Docs** (links use `/organizations/...` matching the actual directory structure):
- All 4 styles: 6983 resolved, 6679 unresolved -- identical. No issue because links don't need a site_prefix.

**VS Code Docs** (links use `/docs/azure/...` with `/docs/` prefix):
| Style | Resolved Links | Unresolved Links |
|-------|---------------|-----------------|
| 1 (relative --dir) | **0** | 2772 |
| 2 (cd into dir) | **0** | 2772 |
| 3 (absolute --dir) | **0** | 2772 |
| 4 (subfolder --dir) | **2514** | 258 |

Only Style 4 correctly resolves links. The difference is 2514 links.

### Backlink Discovery: BUG - Same root cause

**GitHub Docs**: All 4 styles find 35 backlinks for the tracking file. No issue.

**VS Code Docs** (`azure/overview.md`):
| Style | Backlinks Found |
|-------|----------------|
| 1 | 0 |
| 2 | 0 |
| 3 | 0 |
| 4 | **2** |

Only Style 4 discovers backlinks correctly.

### JSON Output Comparison (md5)

**GitHub Docs** (`find --fields links,backlinks --file roles-in-an-organization.md`):
All 4 styles produce **identical** JSON (same MD5 hash).

**VS Code Docs** (`find --fields links,backlinks --file azure/overview.md`):
Styles 1, 2, 3 produce **identical** JSON (all links unresolved, no backlinks).
Style 4 produces **different** JSON (links resolved, 2 backlinks found).

## Root Cause Analysis

The `site_prefix` is derived from the raw `--dir` string value in `main.rs` (line 726):

```rust
let site_prefix_owned = {
    let s = dir.to_string_lossy().replace('\\', "/");
    let s = s.trim().trim_end_matches('/');
    let s = s.strip_prefix("./").unwrap_or(s);
    if s == "." || s.is_empty() {
        None
    } else {
        Some(s.to_owned())
    }
};
```

- `--dir docs` yields `site_prefix = Some("docs")` -- strips `/docs/` from links, resolution works
- `--dir ../vscode-docs/docs` yields `site_prefix = Some("../vscode-docs/docs")` -- can never match `/docs/`, fails
- `--dir ~/.../docs` yields `site_prefix = Some("~/.../docs")` -- same problem
- No `--dir` (CWD is the dir) yields `site_prefix = None` -- no stripping attempted

The site_prefix derivation only works when `--dir` is a bare subfolder name that happens to match the prefix in the links. Any path with separators (relative or absolute) breaks it.

## Proposed Fixes

1. **Extract only the last path component** for site_prefix inference: `dir.file_name()` instead of the full string. This way `/Users/.../docs`, `../vscode-docs/docs`, and `docs` all yield `site_prefix = "docs"`.
2. **Add explicit `--site-prefix` CLI flag** and `.hyalo.toml` `site_prefix` config key for when the directory name doesn't match the link prefix.
3. **Document the behavior** in `--help` output so users know why links aren't resolving.

## Minor Inconsistency: backlinks output format

The `backlinks` command includes a `target` field in each backlink entry plus a `total` count. The `find --fields backlinks` output omits the `target` field and `total` count. Not a bug, but worth noting for API consistency.
