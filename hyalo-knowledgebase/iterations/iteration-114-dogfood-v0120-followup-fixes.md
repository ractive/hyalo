---
title: Iteration 114 — Dogfood v0.12.0 Follow-up Fixes
type: iteration
date: 2026-04-15
status: completed
branch: iter-114/dogfood-v0120-followup-fixes
tags:
  - dogfooding
  - bug-fix
  - permissions
  - mv
  - views
  - links
related:
  - "[[dogfood-results/dogfood-v0120-followup-iter113b]]"
  - "[[iterations/iteration-113-dogfood-v0120-fixes]]"
---

# Iteration 114 — Dogfood v0.12.0 Follow-up Fixes

## Goal

Fix the bugs and UX issues that remained open or were newly discovered in the v0.12.0 follow-up
dogfood session after iter-113/113b/113c landed. See [[dogfood-results/dogfood-v0120-followup-iter113b]]
for the full report.

Scope: data-safety (file permissions), link-rewriting correctness (`mv`), view expressiveness
(orphans/dead-ends), config-layering transparency, and a handful of CLI-UX polish items.

## Bugs

### NEW-BUG-1: File permissions dropped to 0600 on rewrite (HIGH)

Any command that rewrites a file — `hyalo set`, `hyalo task toggle`, and `hyalo mv` when it rewrites
links inside the moved file — replaces the file with one whose mode is `0600`, regardless of the
original mode. `hyalo mv` without link rewrites preserves the mode, which pinpoints the atomic-replace
path.

Reproduction (macOS, `umask 022`):
```
$ ls -l perms.md
-rw-r--r--  … perms.md
$ hyalo set perms.md --property status=planned
$ ls -l perms.md
-rw-------  … perms.md
```

Likely cause: the temporary file created by `tempfile::NamedTempFile` (or equivalent) defaults to
`0600`; `persist()` renames it over the target without copying the original mode.

**Fix**: Before the atomic replace, `fs::set_permissions(tmp, original_metadata.permissions())`
(and fall back to `umask`-derived default if the original metadata can't be read — e.g. a brand-new
file). Ensure tests cover both existing and newly-created target paths.

### NEW-BUG-2: `mv` with self-referencing link errors after move (MEDIUM)

A file containing a markdown link to itself (`[label](self.md)`) is moved, but `hyalo mv` then
errors:

```
Error: could not verify ./self.md is within vault
  cause: failed to canonicalize path: ./self.md
```

Exit code is non-zero, but the rename already happened. The self-link inside the new file is not
rewritten.

**Fix**: When rewriting links inside the *moved* file, resolve link targets against the new path
(`--to`), not the source path. Add an e2e test that moves a file containing a link to itself and
asserts that (a) exit code is 0, (b) the link in the new file points at the new location.

### NEW-BUG-3: Views cannot filter by `orphan` / `dead-end` (MEDIUM)

Views today only support `properties`, `tags`, `task`, and `fields`. Defining `[views.orphans]` with
`fields = ["backlinks"]` merely adds a display column — it does not restrict the result set. In the
own KB, `find --view orphans --count` returns 237 (every file) while `find --orphan --count` returns 83.

**Fix**: Extend the view schema with optional `orphan = true` / `dead_end = true` booleans that map
onto the same filters exposed by `find --orphan` / `find --dead-end`. Update the view validator so
`hyalo lint` can flag legacy view definitions that look orphan-ish but don't filter.

### BUG-6 (still open): `mv` rewrites `/`-prefixed and bare wiki-style links (MEDIUM)

Carried over from [[dogfood-results/dogfood-v0120-multi-kb]]. Reproduced again:

```
hyalo mv games/anatomy/index.md --to games/anatomy-renamed/index.md --dry-run
  [Web Workers](/en-US/docs/Web/API/Web_Workers_API/Using_web_workers)
    → [Web Workers](../../en-US/docs/Web/API/Web_Workers_API/Using_web_workers)
  [obsidian](Note One)
    → [obsidian](../Note One)
```

- Links starting with `/` are site-absolute and must be left untouched.
- Tokens with no `.md` extension and no path separator (e.g. `Note One`) are almost always labels,
  Obsidian-style wikilinks, or external refs — not file paths — and should not be rewritten.

**Fix**: In the link-rewriter, skip any target that (a) starts with `/`, (b) starts with a URL scheme
(`http://`, `https://`, `mailto:`, `tel:`, `#`), or (c) has no `.md` suffix *and* contains no path
separator. Preserve behaviour for genuine relative/absolute filesystem paths.

### NEW-BUG-4: Nested `.hyalo.toml` silently shadowed (LOW)

With:
```
root/.hyalo.toml        # dir = "subkb", declares type "article"
root/subkb/.hyalo.toml  # declares type "chapter"
```
Running `hyalo types list` from `root/` shows only `article`. The nested `subkb/.hyalo.toml` is
silently ignored.

**Fix**: When config resolution selects a parent `.hyalo.toml` whose `dir` points at a directory
containing another `.hyalo.toml`, emit a single one-line warning:

```
warning: ignoring nested config subkb/.hyalo.toml (shadowed by root config)
```

Do not try to merge — keep semantics simple, just make the shadowing visible. Suppress behind
`--no-hints` / `quiet = true`.

### NEW-UX-1: `append --tag` shows clap error instead of hint (LOW)

```
$ hyalo append note.md --tag foo
error: unexpected argument '--tag' found
  tip: to pass '--tag' as a value, use '-- --tag'
```

The `append` help text already explains that `--tag` is not available on append and suggests
`hyalo set --tag T`, but the CLI error does not surface this.

**Fix**: Add a custom clap error handler (or a pre-parse check) for `append` that, when `--tag` is
seen, emits:

```
error: `hyalo append` does not accept --tag (tags are scalar list items, not appendable)
  hint: use `hyalo set <file> --tag <tag>` to add a tag
```

### BUG-3 minor gap: `AND OR` only warns about the first operator (LOW)

`find "AND OR"` warns about `AND` but silently swallows `OR`. Low impact; the user still sees a
warning and fixes the query.

**Fix**: Rework the bare-operator check to concatenate every stripped operator into the warning
message, e.g. `"AND", "OR" were interpreted as boolean operators …`.

## UX / Housekeeping

### UX-5 (still open): Link resolution is case-sensitive

Not addressed in this iteration. Leave as a follow-up — MDN-specific, orthogonal to the fixes above.

### UX-6 (still open): Repeated unclosed-frontmatter warnings

Not addressed in this iteration. The warning is correct but noisy. A future iteration should add an
ignore-list mechanism (e.g. `[scanner] ignore = ["path/to/broken.md"]` in `.hyalo.toml`).

### Data-quality: NEW-BUG-5 — unfixed comma-joined tags in own KB

[[dogfood-results/dogfood-v0120-multi-kb]] claimed "FIXED during this session" for 13 comma-joined
tags in `backlog/done/`. They are still present. Run `hyalo lint --fix` on the own KB as part of
this iteration and update the prior report's claim.

## Tasks

### NEW-BUG-1: preserve file mode on atomic rewrite
- [x] Locate the atomic-replace helper (hyalo-io or equivalent) used by `set`, `task toggle`, and `mv`'s body rewriter
- [x] Before rename/persist, copy the source file's permissions onto the temp file
- [x] If the target is brand-new (no prior file), leave the temp file with the platform default derived from `umask`
- [x] Unit test: write two files with modes 0644 and 0600; mutate both; assert modes are preserved
- [x] e2e test: `hyalo set file.md --property status=x` on a 0644 file → still 0644
- [x] e2e test: `hyalo task toggle file.md --all` preserves mode
- [x] e2e test: `hyalo mv a.md --to b.md` with link rewrite preserves mode of the new file

### NEW-BUG-2: mv-with-self-link should not error after move
- [x] Reproduce in an integration test: create `self.md` containing `[me](self.md)`; `hyalo mv self.md --to other.md`
- [x] Fix the link-rewriter so it canonicalises against the destination path, not the source, after the rename
- [x] Assert exit code 0 and that `other.md` contains `[me](other.md)`

### NEW-BUG-3: view filters for orphan / dead-end
- [x] Add `orphan: Option<bool>` and `dead_end: Option<bool>` to the view schema (TOML + struct)
- [x] Thread them into the same filter pipeline as `find --orphan` / `--dead-end`
- [x] `hyalo lint` flags views that use `fields = ["backlinks"]` as their only narrowing mechanism (likely intended to be `orphan = true`)
- [x] Update own KB `.hyalo.toml`: `[views.orphans] orphan = true` (remove the misleading `fields` line)
- [x] e2e test: a view with `orphan = true` returns the same count as `find --orphan`

### BUG-6: skip site-absolute and non-md link rewrites
- [x] In the link rewriter, add a `should_rewrite(target: &str) -> bool` helper with the rules above
- [x] Skip `/…`, URL schemes, `#…`, `mailto:…`, `tel:…`
- [x] Skip bare tokens with no `.md` suffix and no `/`
- [x] Add test cases for each skip rule (MDN-style, Obsidian-style, fragment-only)
- [x] Regression test: genuine relative link `../notes/x.md` still gets rewritten

### NEW-BUG-4: warn on shadowed nested config
- [x] When config resolution settles on a config whose `dir` points at a subdirectory containing its own `.hyalo.toml`, emit a one-line warning
- [x] Respect `--no-hints` / global quiet flags
- [x] e2e test: create `root/.hyalo.toml (dir=subkb)` + `root/subkb/.hyalo.toml`; assert warning is emitted once

### NEW-UX-1: friendly error for `append --tag`
- [x] Intercept `--tag` in the `append` subcommand's argument parsing
- [x] Emit the hint shown above instead of the generic clap error
- [x] e2e test: `hyalo append file.md --tag foo` exits non-zero with the new message

### BUG-3 follow-up: aggregate operator warning
- [x] Collect all stripped bare operators into the warning message
- [x] e2e test: `find "AND OR"` warns about both

### Data quality
- [x] Run `hyalo lint --fix` to split the 13 comma-joined tags in `backlog/done/`
- [x] Update [[dogfood-results/dogfood-v0120-multi-kb]] — remove the inaccurate "FIXED during this session" claim (or mark it fixed in iter-114)

### Quality gate
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace -q`
- [x] Rebuild release binary and re-run a focused dogfood pass against the own KB + the `/tmp` multi-KB fixture
- [x] Update CLAUDE.md / skill templates if any flag surface changes

## Deferred to a Later Iteration

- UX-5: Case-insensitive link resolution (MDN-specific; probably needs a `--case-insensitive` flag or `.hyalo.toml` option).
- UX-6: Per-file ignore list for unclosed-frontmatter warnings.
- Consider extending `find` so bare-token queries can opt in to case-insensitive regex without slash-form (`--property 'title~=dogfood' --ci`).
