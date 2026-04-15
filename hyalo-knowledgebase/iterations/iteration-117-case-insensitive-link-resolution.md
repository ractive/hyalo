---
title: Iteration 117 — Case-insensitive link resolution
type: iteration
date: 2026-04-15
status: planned
branch: iter-117/case-insensitive-link-resolution
tags:
  - links
  - resolver
  - portability
  - mv
  - find
  - mdn
related:
  - "[[dogfood-results/dogfood-v0120-iter115-followup]]"
  - "[[dogfood-results/dogfood-v0120-iter114-followup]]"
  - "[[dogfood-results/dogfood-v0120-followup-iter113b]]"
  - "[[dogfood-results/dogfood-v0120-multi-kb]]"
  - "[[iterations/iteration-116-dogfood-v0120-iter115-followup]]"
---

# Iteration 117 — Case-insensitive link resolution

## Goal

Close the last three symptoms trailing the v0.12.0 dogfood rounds — BUG-6, NEW-2,
and UX-5 (old) — which all share a single root cause: the link resolver does a
case-sensitive string/filesystem comparison, and external vaults (notably MDN)
use different case conventions for URLs and filesystem paths.

Ship one coherent fix that resolves all three symptoms without breaking
case-sensitive filesystems.

## The issue

**One bug, three symptoms.** MDN's on-disk layout is all lowercase but its
markdown links use PascalCase URLs:

- File on disk: `files/en-us/web/javascript/reference/iteration_protocols/index.md`
- Link in `…/promise/any/index.md`: `/en-US/docs/Web/JavaScript/Reference/Iteration_protocols`

`/Users/james/devel/mdn/.hyalo.toml` currently sets `dir = "files/en-us"` but
does **not** set `site_prefix`. Even if it did (`site_prefix = "en-US/docs"`),
after stripping the prefix the resolver would still be looking up
`Web/JavaScript/Reference/Iteration_protocols.md` — which does not exist; the
file on disk is `web/javascript/reference/iteration_protocols.md`. Hyalo does a
literal comparison, so it reports the link as `(unresolved)` and `mv` reports
0 rewrites.

On macOS's case-insensitive APFS the failure mode is **worse and more
misleading**: `full.is_file()` returns true because the filesystem folds case,
so the link *appears* resolved, but the `path` field emitted by
`find --fields links` is the PascalCase-as-written input — not the real
lowercase on-disk path. Users and downstream tooling see a path that will not
exist when the vault is checked out on Linux.

### Three symptoms currently tracked as separate bugs

| ID | Origin | Symptom |
|----|--------|---------|
| **BUG-6** | [[dogfood-results/dogfood-v0120-multi-kb]] | `hyalo mv` reports 0 rewrites for absolute URL-style links on MDN |
| **NEW-2** | [[dogfood-results/dogfood-v0120-iter115-followup]] | `find --fields links` shows MDN absolute URLs as `(unresolved)` |
| **UX-5 (old)** | [[dogfood-results/dogfood-v0120-iter114-followup]] | No `--case-insensitive` option / no way to get MDN links to resolve |

All three are resolved by a single change to the resolver — they do not need
separate fixes.

### Why a naive `to_lowercase()` breaks things

The three filesystem classes hyalo must support:

| Class | Examples | Can `Foo.md` and `foo.md` coexist? |
|-------|----------|-----------------------------------|
| Case-sensitive | Linux ext4/btrfs/xfs, APFS case-sensitive, HFS+ case-sensitive | **Yes** |
| Case-insensitive, case-preserving | macOS APFS default, HFS+ default, NTFS | No |
| Case-folding | FAT | No |

Risks if we just lowercase both sides of every comparison:

1. **Ambiguous resolution.** A vault with both `Web/JavaScript/Iteration.md`
   and `web/javascript/iteration.md` (legal on Linux) can have a link silently
   resolve to the wrong file — worse than reporting `(unresolved)`.
2. **`mv` damages the wrong file.** Rewrite targets chosen via case-insensitive
   match could rewrite links pointing at an unrelated sibling.
3. **Portability regression.** A macOS-authored KB with sloppy casing could
   produce rewrites that resolve on macOS but break on Linux.
4. **Silent behavior flip.** Existing tests and user expectations rely on
   case-sensitive resolution returning `None`.

## Design

The resolver gets a case-insensitive fallback that is:

- **Exact-match first.** If the literal path hits the filesystem, nothing
  changes. The fallback never runs. All current behavior is preserved.
- **Unique-match only.** The fallback consults a pre-built
  `{ lowercased_rel_path → Vec<real_rel_path> }` index built once per
  command. It resolves iff there is exactly one candidate; on ambiguity it
  returns `None` and (in verbose modes) emits a warning.
- **Canonical-path return.** The returned path is always the real on-disk case,
  not the input case. This fixes the misleading macOS `find --fields links`
  output and makes `mv` rewrite to cross-platform-safe targets.
- **Opt-in with safe default.** New config key
  `[links] case_insensitive = "auto" | true | false` in `.hyalo.toml`. Default
  `"auto"` enables the fallback only when the vault's filesystem is probed as
  case-insensitive (stat a temp file's uppercase variant). Linux users keep
  strict semantics unless they set `true` explicitly.
- **Lint rule to surface mismatches.** Add `link-case-mismatch` warning when
  a link's target resolves only via the case-insensitive fallback. `--fix`
  rewrites the link to the canonical case so a vault can be migrated to strict
  matching over time.

### Signature shape

- `discovery::resolve_target` gains an optional parameter carrying the
  case-insensitive index (or the index is owned by a new `LinkResolver`
  type that wraps canonical dir + optional index).
- Callers already iterate with a canonicalized dir once (see
  `link_rewrite::plan_mv`, `link_fix::scan_for_issues`,
  `commands/find/mod.rs`) — the index is built alongside.
- `LinkGraph::build` already walks the vault; extend it to populate the
  case-insensitive index at the same time. One walk, no extra I/O cost.

### What does not change

- File discovery (`discover_files`) stays case-sensitive — we always report the
  real on-disk path.
- Wikilink stem matching (`[[some note]]`) already normalizes for whitespace
  only; that stays.
- Config loading, schema, everything outside the link resolver.

## Out of scope

- Changing the default to `true` on case-sensitive filesystems. The `auto`
  default is deliberately conservative.
- Rewriting MDN's link corpus to canonical casing (that is a downstream
  user decision enabled by the new `lint --fix` rule).
- Normalizing Obsidian-style short wikilinks (`[[Note Name]]`) against
  filesystem case — they are already resolved via a separate stem-matching
  path and do not go through `resolve_target`.
- Windows path-separator edge cases (already covered by existing backslash
  normalization).

## Tasks

- [ ] Build `CaseInsensitiveIndex { map: HashMap<String, SmallVec<[String; 1]>> }` in `hyalo-core`
- [ ] Populate the index during `discover_files` / `LinkGraph::build`
- [ ] Probe filesystem case-sensitivity once per run (create+stat uppercase temp file under vault dir)
- [ ] Add `[links] case_insensitive` config key with values `"auto" | true | false`, default `"auto"`
- [ ] Thread the index into `discovery::resolve_target` via an optional `Option<&CaseInsensitiveIndex>` parameter
- [ ] Exact-match path unchanged; fallback only when literal match misses and fallback is enabled
- [ ] On ambiguous fallback (>1 candidate), return `None` and record a warning in the resolver
- [ ] Update `link_rewrite::plan_mv` so it rewrites links using the canonical on-disk target path
- [ ] Update `find --fields links` `path` field to always be the canonical on-disk path
- [ ] Update `link_fix` to treat case-insensitive matches as fixable (rewrite to canonical case) under a new `link-case-mismatch` code
- [ ] New lint rule `link-case-mismatch` in `hyalo-lint` with `--fix` support
- [ ] Unit tests: exact match wins, unique fallback, ambiguous fallback returns None, disabled fallback matches today's behavior
- [ ] E2E tests: MDN-shape fixture (lowercase files, PascalCase links, `site_prefix = "en-US/docs"`) — `find --fields links`, `mv`, `backlinks`, `links fix` all work
- [ ] E2E tests: case-sensitive fixture with both `Foo.md` and `foo.md` — ambiguous link stays unresolved
- [ ] E2E tests: `[links] case_insensitive = false` preserves today's strict behavior
- [ ] Probe test: verify the filesystem-case probe picks up macOS APFS default and case-sensitive temp dirs correctly
- [ ] Docs: README link-resolution section, `.hyalo.toml` reference, `find --help` / `mv --help` updates
- [ ] Skill template: document the new config key and lint rule
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -q`
- [ ] Dogfood against MDN with `.hyalo.toml` updated to add `site_prefix = "en-US/docs"`; verify BUG-6, NEW-2, UX-5 all resolved

## Acceptance Criteria

- [ ] On Linux with the MDN fixture, `hyalo find --file <promise>/any/index.md --fields links` returns the real lowercase `path` for every PascalCase URL that has a unique lowercase match
- [ ] On macOS with the MDN fixture, the same command returns the real lowercase `path`, not the PascalCase input
- [ ] `hyalo mv web/javascript/reference/iteration_protocols/index.md --to …/foo.md --dry-run` reports the rewrite on links written as `/en-US/docs/Web/JavaScript/Reference/Iteration_protocols`, and the rewritten link uses the canonical path
- [ ] On a case-sensitive vault with both `Foo.md` and `foo.md`, a link `Bar` that only matches case-insensitively to one of them stays unresolved (no silent ambiguous pick)
- [ ] Setting `[links] case_insensitive = false` in `.hyalo.toml` reproduces today's strict-matching behavior byte-for-byte on all existing tests
- [ ] `hyalo lint --fix` rewrites `link-case-mismatch` warnings to the canonical path, and re-running `lint` reports zero warnings
- [ ] All three originally-reported symptoms (BUG-6, NEW-2, UX-5-old) are dogfood-verified against MDN and marked FIXED in the relevant dogfood reports
