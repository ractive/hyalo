---
title: "Dogfood v0.12.0 — Post Iteration 113 Verification"
type: research
date: 2026-04-14
status: active
tags:
  - dogfooding
  - verification
  - bug-fix
related:
  - "[[dogfood-results/dogfood-v0120-multi-kb]]"
  - "[[iterations/iteration-113-dogfood-v0120-fixes]]"
---

# Dogfood v0.12.0 — Post Iteration 113 Verification

Re-ran the scenarios from the [[dogfood-results/dogfood-v0120-multi-kb]] report after iteration 113 merged.
Binary: `hyalo 0.12.0` installed via `cargo install --path crates/hyalo-cli`.

## Bug Fix Verification

### BUG-1: `--dir` config lookup for `types set` / `views set` — PARTIALLY FIXED

**External KB with `--dir` flag**: FIXED. `types set` and `views set` now correctly write to
the external KB's `.hyalo.toml` when `--dir` is passed explicitly. `types show` and `views list`
also read from the correct location.

**REGRESSION: Root config with `dir = "..."` setting**: When the root `.hyalo.toml` has
`dir = "hyalo-knowledgebase"` and you run `types set` (without `--dir`), the new type is written
to `hyalo-knowledgebase/.hyalo.toml` instead of the root `.hyalo.toml`. This creates a split
config: types/views defined in root config become invisible to commands that resolve config to
the KB subdirectory.

**REGRESSION: `views list` returns empty**: All view commands (`views list`, `views set`,
`views remove`, `find --view`) are broken for the default case. They resolve config to
`hyalo-knowledgebase/.hyalo.toml` (which doesn't exist), while views are defined in the root
`.hyalo.toml`. `types list` still works because schema loading uses a different code path
(loaded by hyalo-core during vault init, not by CLI commands).

**Root cause**: The BUG-1 fix changed `views.rs` and `types.rs` to use `resolve_toml_path(dir)`
which joins the KB directory with `.hyalo.toml`. But when `dir` comes from `dir = "hyalo-knowledgebase"`
in the root config, the config file is the *root* `.hyalo.toml`, not one inside the KB directory.
The fix should resolve to whichever `.hyalo.toml` was used to *find* the `dir` setting.

### BUG-2: TOML section reordering — FIXED

Added a new type via `types set` to an external KB. The existing sections (article type, views)
remained in their original order, and the new type was appended after existing types. No
alphabetical reordering, no escape-style changes. The `toml_edit` migration is working correctly.

### BUG-3: Bare boolean operator warning — FIXED

- `find "and"` → warning: `"and" was interpreted as a boolean operator, leaving an empty query`
- `find "or"` → same warning with suggestion to use quoted form
- Both suggest: `To search for the literal word, quote it: '"and"'`

**Edge case**: `find "not"` returns all files without warning. `NOT` alone acts as negation with
no operand, matching everything. Minor — could warn, but not a blocker.

### BUG-4: `task toggle --all` with deep indentation — FIXED

Tested with checkboxes at 0, 2, 4, 6, 8, 12, and 16-space indentation.
`task toggle --all --dry-run` found all 7 checkboxes. Previously it missed anything
beyond a few levels of indentation.

### BUG-5: `create-index --index=PATH` — NOT FIXED

`hyalo create-index --dir /tmp/ext-kb/ --index=/tmp/custom-path` still wrote to
`/tmp/ext-kb/.hyalo-index` instead of `/tmp/custom-path`. The result JSON confirmed:
`"path": "/tmp/ext-kb/.hyalo-index"`. The `--index` flag is being ignored.

### BUG-8: `remove --tag` with malformed comma-tags — FIXED

`hyalo remove file.md --tag "cli,ux"` successfully removed the malformed comma-tag.
Previously this failed with `"invalid character ',' in tag name"`.

## UX Improvement Verification

### UX-2: `lint --type` shortcut — WORKING

`hyalo lint --type iteration` correctly scanned only the 12 iteration-type files
(matching the `iteration` schema). Previously this flag didn't exist and had to use `--glob`.

### UX-3: Lint detects comma-joined tags — WORKING

`hyalo lint` now warns: `tag "cli,ux,find" appears to be comma-joined -- should be separate list items`.
Detected all 13 comma-tag files in own KB. `lint --fix --dry-run` shows `split-comma-tags` as
a fixable action with the proposed split values.

### UX-4: `task toggle --dry-run` — WORKING

`task toggle --all --dry-run` correctly shows what would be toggled without modifying the file.

## Performance

Own KB (236 files, no index):
- `summary`: 33ms
- FTS `"snapshot index architecture"`: 55ms
- Both essentially unchanged from pre-iter-113

## New Issues Found During Verification

### ~~NEW-BUG-1: Config resolution regression (HIGH)~~ — FIXED in iter-113b

`types set` and `views set/list/remove` resolved `.hyalo.toml` to `<dir>/.hyalo.toml` instead of
using the config file that defined the `dir` setting. Fixed by tracking `config_dir` (where
`.hyalo.toml` was loaded from) separately from `dir` (the vault directory).

### BUG-5 correction: NOT A BUG

The `create-index` command uses `--output` (`-o`), not `--index`. The `--index` flag is a global
argument for other commands to *use* a pre-built index. The dogfood report used the wrong flag.

### `find "not"` behavior: BY DESIGN (LOW)

`NOT` is not a keyword operator — negation uses the `-` prefix syntax (e.g., `-foo`). The word
"not" is treated as a regular search term. There is a test explicitly asserting this behavior.

### Still open from original report

- **BUG-6**: `mv --dry-run` rewrites absolute URL-style links (not addressed in iter-113)
- **UX-5**: Link resolution case-sensitivity (MDN-specific, not addressed)
- **UX-6**: Repeated frontmatter warning (not addressed)

## Summary

| Bug | Status | Notes |
|-----|--------|-------|
| BUG-1 | Partial fix / regression | `--dir` flag works; `dir=` config setting broken for views |
| BUG-2 | Fixed | toml_edit preserves order |
| BUG-3 | Fixed | Warning shown for bare AND/OR |
| BUG-4 | Fixed | Indentation-agnostic regex |
| BUG-5 | Not a bug | Flag is `--output`, not `--index` |
| BUG-8 | Fixed | Comma-tags removable |
| UX-2 | Working | `lint --type` |
| UX-3 | Working | Comma-tag detection |
| UX-4 | Working | `task toggle --dry-run` |

All critical bugs from iter-113 have been addressed in iter-113b.

**Superseded by [[iterations/iteration-118-split-index-flag]]:** `--index=PATH` is now `--index-file=PATH`; bare `--index` is a boolean flag.
