---
title: "Dogfooding v0.4.2 — Consolidated Report"
type: research
date: 2026-03-28
status: completed
tags:
  - dogfooding
  - performance
  - testing
---

# Dogfooding v0.4.2 — Consolidated Report

Tested on **../docs/content** (3521 md files) and **../vscode-docs/docs** (339 md files).

## Bugs Found

### BUG 1: Orphan count discrepancy between disk scan and index scan (CRITICAL)

**Repo:** vscode-docs/docs (339 files)

- `hyalo summary` (disk scan): **48 orphans**
- `hyalo summary --index .hyalo-index`: **25 orphans**
- **23 orphans silently disappear** when using the snapshot index

Missing orphans include: `cpp/enable-logging-cpp.md`, `cpp/natvis.md`, `csharp/signing-in.md`, `csharp/testing.md`, `copilot/guides/mcp-developer-guide.md`, `datascience/data-wrangler.md`, `enterprise/policies.md`, `getstarted/copilot-quickstart.md`, `intelligentapps/agent-inspector.md`, `intelligentapps/tracing.md`, `intelligentapps/reference/FileStructure.md`, `intelligentapps/reference/SetupWithoutAITK.md`, `intelligentapps/reference/TemplateProject.md`, `java/java-linting.md`, `java/java-refactoring.md`, `languages/powershell.md`, `languages/tsql.md`, `nodejs/nodejs-deployment.md`, `python/python-on-azure.md`, `setup/network.md`, `sourcecontrol/repos-remotes.md`, `supporting/oss-extensions.md`.

**Root cause hypothesis:** The index link graph may be computing reachability differently (possibly including more link types or resolving links differently) than the disk-scan path.

### BUG 2: `--limit 0` returns all files instead of none (MEDIUM)

`hyalo find --dir ../docs/content --limit 0` returns ALL 3520 files (330,981 lines of JSON).

**Expected:** Either return `[]` (zero results) or reject with an error.
**Actual:** Behaves as if `--limit` is unset/unlimited.
**Impact:** Violates principle of least surprise; could cause OOM in pipes.

### BUG 3 (UX): `--fields all` not supported (LOW)

`hyalo find --fields all` returns: `unknown field "all"`.

The help text says "default: all except properties-typed and backlinks", so users naturally try `--fields all`. Should be easy to add as a keyword.

### BUG 4 (UX): `--fields title` not supported (LOW)

`title` is the most commonly desired field for browsing, but requires `--fields properties` which returns the entire property bag. A `title` shorthand would improve ergonomics.

## UX Wishes (not bugs)

1. **No `--sort title` or `--sort date`** — Only `file`, `modified`, `backlinks_count`, `links_count` are supported. Sorting by a frontmatter property value (especially title or a date field) would be valuable.
2. **No `--body` flag** — Body search is a positional argument, which is non-obvious. `--body "text"` would be more discoverable.
3. **Warning noise** — `skipping code-security/concepts/index.md: unclosed frontmatter` appears on every full-scan command. Consider `--quiet` or show warnings once.
4. **`--filter` typo suggests `--file` not `--property`** — When `--filter` is used, clap suggests `--file`. Should also suggest `--property`.
5. **Default `find` output is very verbose** — Without `--fields`, default includes links, sections, tasks, properties, tags. For human use, a compact default would help (though `--format text` mitigates this).
6. **`--sort modified` useless on git-cloned repos** — All files share the same mtime. Supporting sort by frontmatter date property would help.

## Performance Results

### Index vs No-Index (50 operations on 3521 files)

| Metric | Without Index | With Index | Speedup |
|---|---|---|---|
| **Total (50 ops)** | **9,949 ms** | **4,713 ms** | **2.1x** |

#### Per-command breakdown

| Command | Count | No Index (avg) | With Index (avg) | Speedup |
|---|---|---|---|---|
| summary | 10 | 258 ms | 61 ms | **4.2x** |
| find --fields properties | 10 | 66 ms | 56 ms | 1.2x |
| find --property (filter) | 5 | 64 ms | 54 ms | 1.2x |
| find --sort modified | 5 | 401 ms | 172 ms | **2.3x** |
| tags | 5 | 178 ms | 55 ms | **3.2x** |
| properties | 5 | 177 ms | 57 ms | **3.1x** |
| backlinks | 5 | 184 ms | 55 ms | **3.3x** |
| find --fields sections | 5 | 63 ms | 54 ms | 1.2x |

**Key insight:** Commands needing full-vault scan (summary, tags, properties, backlinks) see 3-4x speedup. Simple `find` with `--limit` already returns in ~60ms (process startup is the bottleneck), so index adds little there.

### hyalo vs rg Comparison (3521 files)

| Test | hyalo | rg/find | Ratio | Notes |
|---|---|---|---|---|
| Content search "kubernetes" | 0.219s | 0.051s | 4.3x slower | hyalo returns rich context |
| Content search "authentication" | 0.233s | 0.060s | 3.9x slower | |
| Property filter (contentType=how-tos) | 0.159s | 0.062s | 2.6x slower | hyalo parses YAML correctly |
| Property regex (title~=API) | 0.165s | 0.056s | 2.9x slower | rg can match body lines too |
| All files listing | 0.175s | 0.044s | 4.0x slower | |
| Summary | 0.229s | 0.044s | 5.2x slower | hyalo gives full stats |
| **With index: property filter** | **0.040s** | 0.062s | **0.65x (faster!)** | |
| **With index: property regex** | **0.038s** | 0.056s | **0.68x (faster!)** | |
| **With index: summary** | **0.046s** | 0.044s | **1.05x (tied)** | |

**Bottom line:** Without index, hyalo is 2.5-5x slower than rg (expected — it parses YAML). **With index, hyalo beats rg** for property queries and ties for summary. Index creation cost (307ms) amortizes after 2-3 queries.

### vscode-docs Index Speedup

| Command | Disk scan | With index | Speedup |
|---|---|---|---|
| summary | 104 ms | 11 ms | **9.5x** |

### Performance vs v0.4.1

| Command (docs/content) | v0.4.2 | v0.4.1 | Change |
|---|---|---|---|
| summary | 0.229s | 0.217s | ~same |
| find --fields properties | 0.175s | 0.175s | same |
| find --fields links,backlinks | 0.54s | 0.494s | ~same |
| backlinks | 0.19s | 0.156s | ~same |

No regressions. Performance is stable from v0.4.1.

## Edge Case Testing (30 tests)

**29/30 passed.** Only `--limit 0` was buggy (see BUG 2 above).

Highlights:
- **Error handling:** All error cases produce clear JSON error messages with exit code 1 or 2
- **Unicode:** Body search for Japanese characters works; unicode property filters work
- **Corrupt index:** Graceful degradation with warning, falls back to disk scan
- **Missing index:** Warning + fallback (no crash)
- **Duplicate fields:** De-duplicated correctly
- **Multiple --property flags:** AND semantics, works correctly
- **Large output:** 3520 files with all properties in 0.16s (3.27 MB); all links in 0.37s (3.57 MB)
- **Pipe behavior:** Valid JSON on stdout, warnings on stderr — correct fd separation

## Mutation Testing (10 tests)

**All 10 passed.** Tested: set, remove, append, mv, tags rename, properties rename, index+mutation, bulk set via glob, and 3 error cases. All mutations produce structured JSON output with modification counts.

## Overall Assessment

v0.4.2 is solid. One real bug (orphan count discrepancy in index — CRITICAL), one semantic issue (`--limit 0`), and several UX polish items. Performance is excellent and stable. The snapshot index is the standout feature — it makes hyalo competitive with or faster than rg for frontmatter queries, while providing dramatically richer output.
