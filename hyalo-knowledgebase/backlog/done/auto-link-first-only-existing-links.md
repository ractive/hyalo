---
title: "links auto: --first-only ignores existing links to the target"
type: backlog
date: 2026-07-04
tags:
  - backlog
  - links
  - auto-link
  - bug
status: completed
priority: medium
origin: external-user dogfood 2026-07-04 (third-party vault)
---

# links auto: --first-only ignores existing links to the target

## Problem

`--first-only` keeps only the lowest-offset **candidate** per
(source file, target title) — see `auto_link.rs` step 4b — but existing
`[[wikilinks]]` to the same target don't consume that slot. Existing links are
exclusion *zones* (their text isn't re-linkified), yet a plain-text mention of
the same title elsewhere in the file is still treated as the "first" mention
and gets linked.

Observed by an external user: a sentence already containing `[[fake-login]]`
gained a second link on the adjacent plain mention, producing

> the `[[fake-login]]` envVars block from `[[fake-login]]`

in one sentence — with `--first-only` active. The user reverted the edit.

## Proposal

When `--first-only` is active, a target that already has at least one existing
wikilink anywhere in the source file should produce **no** new matches in that
file (the existing link *is* the first mention). Implementation sketch: the
scan already extracts existing link spans per file for exclusion-zone
handling; feed the set of already-linked targets per file into the step-4b
keep-mask so `seen` starts pre-populated.

Open design question: should this be the unconditional behavior of
`--first-only` (recommended — matches the flag's intent, "link each target at
most once per file"), or a separate `--skip-linked-targets` flag? Note the
current behavior is what the flag's help text arguably promises against:
"Only emit the first mention of each target per source file".

## Acceptance criteria

- [x] File already containing `[[target]]`: `--first-only` emits zero matches for that target in that file
- [x] File without an existing link: behavior unchanged (first plain mention linked)
- [x] Case-insensitive: existing `[[Fake-Login]]` suppresses mentions of "fake-login" per the vault's case mode
- [x] Aliased existing links (`[[target|label]]`) count as existing links to the target
- [x] Help text updated to state the semantics explicitly; e2e test for the adjacent-mention case
