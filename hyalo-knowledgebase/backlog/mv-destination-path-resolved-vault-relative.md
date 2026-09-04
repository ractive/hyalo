---
type: backlog
title: "mv resolves the source path against CWD but the destination against the vault, nesting the vault dir"
date: 2026-09-04
status: planned
priority: medium
origin: "plan consolidation session after the 261–268 loop, 2026-09-04"
---

# mv resolves the source path against CWD but the destination against the vault

## Repro

From the repo root (CWD = parent of the configured vault `hyalo-knowledgebase/`):

```text
$ hyalo mv hyalo-knowledgebase/iterations/iteration-270-md034-boundary-br-tag.md \
           hyalo-knowledgebase/iterations/iteration-270-schema-write-semantics.md
Moved iterations/iteration-270-md034-boundary-br-tag.md → hyalo-knowledgebase/iterations/iteration-270-schema-write-semantics.md
$ git status --short
 D hyalo-knowledgebase/iterations/iteration-270-md034-boundary-br-tag.md
?? hyalo-knowledgebase/hyalo-knowledgebase/
```

The source was accepted with the vault-directory prefix and normalised to the
vault-relative `iterations/…` (visible in the "Moved" line), but the destination kept its
prefix and was treated as vault-relative, so the file landed at
`hyalo-knowledgebase/hyalo-knowledgebase/iterations/…` — a freshly created nested copy
of the vault directory. Every other command (`find --file`, `lint --file`, `set`, `read`)
accepts the CWD-prefixed form for its path arguments, so a caller who uses the same form
for both `mv` arguments gets a silently wrong move.

## Expected

Source and destination go through the same path normalisation: a destination that is
CWD-relative and lies inside the vault is stripped to vault-relative exactly like the
source, and a destination outside the vault is refused (it already is for
`--files-from`/`--file` via `files_skipped_outside_vault`).

## Notes

- Single-file `mv` positional destination form; `--to` and batch `mv` are unverified and
  should be covered by the same fix and tests.
- No data loss: the file was created at the wrong place, the original was removed. A
  vault with a stale index would also carry the wrong path until rebuilt.
- Candidate home: fold into [[iterations/iteration-269-mv-frontmatter-link-scan-gap]] as a
  fourth part if it is small, since that iteration already touches `mv`.
