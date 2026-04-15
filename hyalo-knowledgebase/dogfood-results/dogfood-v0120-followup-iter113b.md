---
title: "Dogfood v0.12.0 — Follow-up After Iter-113/113b/113c"
type: research
date: 2026-04-15
status: active
tags:
  - dogfooding
  - verification
  - bug-fix
  - permissions
  - views
  - mv
related:
  - "[[dogfood-results/dogfood-v0120-post-iter113]]"
  - "[[dogfood-results/dogfood-v0120-multi-kb]]"
  - "[[iterations/iteration-113-dogfood-v0120-fixes]]"
---

# Dogfood v0.12.0 — Follow-up After Iter-113/113b/113c

Re-ran scenarios from [[dogfood-results/dogfood-v0120-post-iter113]] against the current tip of `main`
(`target/release/hyalo 0.12.0`, built with `cargo build --release`). Covered both the own KB and a
synthetic multi-KB setup under `/tmp/dogfood-v0120-iter113b/` with:

- `ext-kb/` — flat external KB (`dir = "."`)
- `root-kb/` with root `.hyalo.toml` having `dir = "subkb"` and schema/views defined at the root level
- `root-kb/subkb/.hyalo.toml` — a second config inside the vault directory

## Bug Fix Verification

### BUG-1: `--dir` + `dir=` config resolution — FIXED

Root `.hyalo.toml`:

```toml
dir = "subkb"
[schema.types.article]
required = ["title", "date"]
[views.recent]
properties = ["status=active"]
```

- `hyalo views list` shows `recent`
- `hyalo types list` shows `article`
- `hyalo find --view recent` returns `subkb/a1.md`, `subkb/a2.md`
- `hyalo types set recipe --required title,date` appends to **root** `.hyalo.toml` (not `subkb/.hyalo.toml`)
- `hyalo --dir subkb types set …` writes to `subkb/.hyalo.toml`

The iter-113b fix (tracking `config_dir` separately from `dir`) is working correctly in both directions.

### BUG-2: TOML section ordering — FIXED

`types set` / `types remove` preserve existing key order and only mutate the touched sections. No alphabetical reshuffle, no escape-style churn.

### BUG-3: Bare boolean operator warning — PARTIAL

- `find "and"` / `find "or"` / `find "AND"` → warning with `"and" was interpreted as a boolean operator …`
- `find "AND OR"` → warns about `"AND"` only; `OR` is silently swallowed after `AND` short-circuits. Minor.
- `find "not"` / `find "NOT"` → no warning (by design; `NOT` is not an operator in this parser).

### BUG-4: `task toggle --all` deep indentation — FIXED

Exercised 0/2/4/6/8/12/16-space indent plus a tab-indented checkbox. All 8 detected and toggled.

### BUG-5: `create-index --output PATH` — WORKING (with vault guard)

`create-index --output /tmp/custom-index` is rejected with:
```
Error: output path is outside the vault boundary
  hint: use --allow-outside-vault to override
```
That hint is discoverable and safer than the previous silent fallback. Fine as-is.

### BUG-8: `remove --tag` with comma tags — FIXED

`hyalo remove tasks.md --tag "foo,bar"` reports `foo,bar: 0/1 modified` (or removes it if present) without rejecting the name.

`append --property tags=` (empty RHS) now errors cleanly:
```
Error: append --property tags= requires a non-empty tag value
  hint: example: hyalo append --property tags=my-tag --file note.md
```

### UX-2 / UX-3 / UX-4 — STILL WORKING

- `lint --type iteration` scans only iteration files.
- `lint` flags 13 comma-joined tags in `backlog/done/`. `lint --fix --dry-run` reports `split-comma-tags` for each.
- `task toggle --dry-run` shows planned changes without modifying the file.

## New Issues

### NEW-BUG-1: File permissions dropped on rewrite (HIGH)

Any command that rewrites frontmatter or body (`hyalo set`, `hyalo task toggle`, `hyalo mv` *when link rewriting fires*) produces a file with mode `0600` regardless of the source mode. `hyalo mv` without link rewrites preserves the original mode.

Reproduction with default `umask 022` on macOS:

```
$ ls -l perms-test.md
-rw-r--r--  1 james  wheel  108 perms-test.md
$ hyalo set perms-test.md --property status=planned
$ ls -l perms-test.md
-rw-------  1 james  wheel  111 perms-test.md
```

Implication: dogfooding against a git-checked-out vault changes `git status` to show mode-only diffs on any mutated file (`100644 → 100600`), and in a multi-user vault this locks out other readers. Likely cause is `tempfile::NamedTempFile` defaulting to `0600` followed by `persist()` without restoring the original mode.

Suggested fix: before atomic replace, `fs::set_permissions(tmp, original_metadata.permissions())`.

### NEW-BUG-2: `mv` with self-referencing link fails after the move succeeds (MEDIUM)

A file containing a markdown link to itself (`[label](same-file.md)`) is moved correctly, but `hyalo mv` then errors:

```
Error: could not verify ./perms-test4.md is within vault
  cause: failed to canonicalize path: ./perms-test4.md
```

The rename already happened, and the renamed file is present; but the exit code is non-zero and the link inside the new file is not rewritten. Atomicity is broken — the user sees both "success move" output and a follow-up error. Likely the link-rewriter canonicalizes the old path (now missing) after renaming.

### NEW-BUG-3: `views "orphans"` view cannot actually filter orphans (MEDIUM, UX)

Own KB defines:
```toml
[views.orphans]
fields = ["backlinks"]
```
But `fields` only controls display columns. `find --view orphans --count` returns **237** (all files), while `find --orphan --count` returns **83**. There is no view-level `orphan = true` / `dead_end = true` option. Either:

1. Teach views about `orphan` / `dead-end` filters (preferred).
2. Have `hyalo init` or `lint` flag misconfigured views whose filters could never narrow the set.

Workaround today: drop the view and use the `--orphan` flag directly.

### NEW-BUG-4: Nested `.hyalo.toml` silently ignored (LOW)

With the layout:
```
root-kb/.hyalo.toml      # dir = "subkb", declares type "article"
root-kb/subkb/.hyalo.toml   # declares type "chapter"
```
`hyalo types list` (from `root-kb/`) lists only `article`. `chapter` is invisible. No warning or hint that a second `.hyalo.toml` was skipped.

Expected: either merge (like nested git configs) or warn that `subkb/.hyalo.toml` is shadowed. Today it is silent.

### NEW-BUG-5: Prior dogfood's "FIXED during this session" was never committed (LOW, data-quality)

[[dogfood-results/dogfood-v0120-multi-kb]] line 144 says the 10 comma-joined tags in `backlog/done/` were *fixed* during the previous session. They weren't: `hyalo lint` still reports all 13 files (the count even grew). Either run `hyalo lint --fix` or update the prior report.

`priority` still has mixed types (6 number / 84 text) as noted previously.

### NEW-UX-1: `append --tag` gives a clap error, not a hint (LOW)

`hyalo append note.md --tag foo` yields:
```
error: unexpected argument '--tag' found
  tip: to pass '--tag' as a value, use '-- --tag'
```
The `append --help` text does say "Note: --tag is not available on append … Use `hyalo set --tag T`". A nicer error would surface that hint directly instead of clap's generic message.

## Still Open From Prior Reports

### BUG-6: `mv --dry-run` rewrites absolute URL-style links — STILL BROKEN

```
$ hyalo mv games/anatomy/index.md --to games/anatomy-renamed/index.md --dry-run
[dry-run] Moved games/anatomy/index.md → games/anatomy-renamed/index.md
  games/anatomy-renamed/index.md: [Web Workers](/en-US/docs/Web/API/Web_Workers_API/Using_web_workers)
    → [Web Workers](../../en-US/docs/Web/API/Web_Workers_API/Using_web_workers)
```

Absolute URL-style links (those starting with `/`) should be treated as site-absolute and left alone. Obsidian-style bare labels (`[obsidian](Note One)` with no `.md` extension and spaces) are *also* rewritten into relative paths, which is likely wrong too — these are either Obsidian wikilink-ish references or plain anchor text and should not become filesystem paths.

### UX-5 / UX-6 — NOT ADDRESSED

- Link resolution is still case-sensitive (MDN-specific; no `--case-insensitive` option).
- Unclosed-frontmatter files warn on *every* command invocation (`summary`, `find`, etc.). Confirmed with a synthetic `broken-fm.md`: every command prints `warning: skipping broken-fm.md: unclosed frontmatter`. Would still benefit from a per-file suppression or `.hyalo-ignore`-style list.

## Performance (Own KB, 237 files)

| Scenario              | Time       |
|-----------------------|-----------|
| `summary` (text)      | ~25 ms     |
| FTS `"snapshot index architecture"` | ~55 ms |
| FTS `"bm25 ranking"` (no index) | ~60 ms |
| `create-index`        | ~45 ms     |
| FTS with index        | ~55 ms (too small to see speedup) |

No regressions vs. iter-113. KB is too small to exercise the index code path meaningfully; MDN/GH-docs would be needed for that.

## Summary

| Bug / Issue | Status | Notes |
|-----|--------|-------|
| BUG-1 | Fixed | Root-config `dir=` + explicit `--dir` both route correctly |
| BUG-2 | Fixed | toml_edit preserves order |
| BUG-3 | Fixed (minor gap) | `AND OR` only warns about the first operator |
| BUG-4 | Fixed | 16-space & tab indent work |
| BUG-5 | Working (vault-guarded) | Use `--allow-outside-vault` to override |
| BUG-6 | **Open** | `/` URL-style + obsidian-style links still rewritten |
| BUG-8 | Fixed | Comma-tags removable |
| UX-2 | Working | `lint --type` |
| UX-3 | Working | Comma-tag detection |
| UX-4 | Working | `task toggle --dry-run` |
| UX-5 | **Open** | Link case-sensitivity |
| UX-6 | **Open** | Repeated frontmatter warnings |
| NEW-BUG-1 | **Open (HIGH)** | Set/task-toggle/mv-with-rewrite drop mode to 0600 |
| NEW-BUG-2 | **Open (MEDIUM)** | `mv` errors after move when file links to itself |
| NEW-BUG-3 | **Open (MEDIUM)** | Views can't express orphan/dead-end filters |
| NEW-BUG-4 | **Open (LOW)** | Nested `.hyalo.toml` silently shadowed |
| NEW-BUG-5 | **Open (LOW)** | Own KB still has 13 comma-joined tags (prior report claim was wrong) |
| NEW-UX-1 | **Open (LOW)** | `append --tag` shows clap error instead of "use `set --tag`" hint |

## Suggested Iteration Priority

1. **HIGH**: Preserve file mode across atomic rewrites (NEW-BUG-1) — silent permission tightening is a data/ops hazard.
2. **MEDIUM**: Fix `mv` rewriter to handle self-referencing links (NEW-BUG-2) and leave `/`-prefixed + bare Obsidian links alone (BUG-6).
3. **MEDIUM**: Extend views schema with `orphan` / `dead_end` (NEW-BUG-3).
4. **LOW**: Warn when a nested `.hyalo.toml` is shadowed (NEW-BUG-4).
5. **LOW**: Improve `append --tag` error to point at `set --tag` (NEW-UX-1).
6. **LOW / housekeeping**: Run `hyalo lint --fix` on own KB to finally clean the comma-joined tags (NEW-BUG-5).
7. **LOW**: Suppress repeated frontmatter warnings (UX-6).
