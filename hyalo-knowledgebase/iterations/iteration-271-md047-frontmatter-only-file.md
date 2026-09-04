---
type: iteration
title: "Iteration 271 — MD047 false positive on a frontmatter-only file"
date: 2026-09-04
status: planned
tags:
  - iteration
  - lint
  - dogfooding
branch: iter-271/md047-frontmatter-only-file
priority: 11
related:
  - "[[iterations/iteration-267-help-hints-text-polish]]"
  - "[[dogfood-results/dogfood-v0220-obsidian-vaults]]"
---

# Iteration 271 — MD047 false positive on a frontmatter-only file

## Goal

Carry-over from [[iterations/iteration-267-help-hints-text-polish]] (PR #310
test plan, "Reviewer note, out of scope"; flagged during the fill-in loop
that iteration's own `new --dry-run` work targets — a scaffolded file with no
body yet is exactly the shape that trips this): a file whose content is
**only** a frontmatter block — `---\n<yaml>\n---\n`, ending with a single
trailing `\n` after the closing fence — trips `MD047` ("File is missing a
trailing newline"), even though the file plainly ends with one. Reproduces on
a hand-written file too (not specific to `hyalo new`'s scaffold), so this
predates iteration 267 and is a pre-existing bug in hyalo's own lint engine,
not something introduced by that iteration's placeholder changes.

Confirmed repro (2026-09-04, this session):

```text
$ printf -- '---\ntitle: X\ntype: note\n---\n' > vault/a.md
$ hyalo lint --file vault/a.md --jq '.results.files'
[{"file":"a.md", "rule_groups":[{"rule":"MD047","severity":"warn", ...,
  "violations":[{"message":"File is missing a trailing newline", ...}]}]}]
```

Most likely cause: the markdown BODY handed to the mdbook-lint engine after
stripping the frontmatter block is the empty string (0 bytes) for a
frontmatter-only file, and MD047's check (or hyalo's own CRLF-handling
wrapper around it — see `crates/hyalo-mdlint/src/engine.rs` around the
"MD047 must handle CRLF terminators" section) treats an empty body as
trivially "missing a trailing newline" rather than as "nothing to check".
Needs confirming against the actual upstream/wrapper code before deciding
where the fix belongs.

Constraint: **no new CLI flags** from dogfood pressure (project rule). This
is a correctness fix to an existing default-on rule's edge-case handling, not
new surface — `hyalo lint-rules set MD047 --enabled false` is already the
opt-out and stays that way.

## Tasks

### FIX-1: don't flag a frontmatter-only file as missing a trailing newline

- [ ] Confirm the repro with a minimal fixture (frontmatter-only, single
      trailing `\n`) and a second fixture (frontmatter-only, NO trailing
      newline after the closing `---`) to establish the two cases MD047
      needs to tell apart — "empty body after valid frontmatter" must not
      fire, but a genuinely truncated file (frontmatter block itself missing
      its own trailing newline) is a real HYALO005/parse concern, not this
      rule's job either way.
- [ ] Root-cause in `crates/hyalo-mdlint/src/engine.rs`: does the bug live in
      hyalo's own frontmatter-stripping (handing MD047 an empty `body` when
      it should hand it nothing / skip the rule), in hyalo's CRLF-handling
      MD047 wrapper, or upstream in `mdbook-lint-rulesets`' MD047 itself? The
      existing "single-line body carries no terminator to sample" comment
      near iteration H-1c/single-line handling suggests this class of
      edge case has been hit before for a different shape — check whether
      the same guard should extend to a zero-line body.
- [ ] Implement: an empty markdown body (frontmatter-only file, or a
      genuinely empty file) must not fire MD047. Prefer skipping the rule
      entirely for a 0-byte body over trying to fabricate a "correct"
      diagnosis, matching the rule's own purpose (there is no missing
      newline when there is no content to end).
- [ ] Unit test in `crates/hyalo-mdlint/src/engine.rs` for a frontmatter-only
      body (empty after stripping) plus the pre-existing single-line-body
      test near it (regression guard — do not re-break that fix while
      touching the same area).
- [ ] e2e in `crates/hyalo-cli/tests/e2e`: `hyalo new --type note --file
      x.md` (a fresh scaffold with no body content beyond frontmatter, the
      shape the fill-in loop starts from) followed by `hyalo lint --file
      x.md` reports no MD047 violation.
- [ ] Docs: `crates/hyalo-mdlint/src/engine.rs::DESCRIPTION_SUFFIX` or the
      MD047 entry in `hyalo-knowledgebase/docs/schema-and-lint.md` if the fix
      changes what MD047 is documented to check; changelog entry.

## Acceptance criteria

- [ ] `hyalo new --type note --file notes/x.md && hyalo lint --file
      notes/x.md --jq '.results.violations'` → `0`, and the file's bytes are
      unchanged by `--fix` (nothing to fix).
- [ ] A hand-written frontmatter-only file (`---\nkey: v\n---\n`, single
      trailing newline) does not trigger MD047; a file whose frontmatter
      closing fence itself lacks a trailing newline is unaffected by this
      fix (out of scope here — a different, pre-existing shape).
- [ ] Existing MD047 fixtures (CRLF handling, single-line body, multi-fix
      convergence) in `crates/hyalo-mdlint/src/engine.rs` still pass
      unchanged.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`, `hyalo lint --strict` on the KB,
      xtask help-drift check.
- [ ] Changelog entry via `hyalo changelog add`; a DEC recorded in
      [[decision-log]] if the fix's scope (skip-when-empty vs. a narrower
      condition) is non-obvious.

## Links

- [[iterations/iteration-267-help-hints-text-polish]]
- [[dogfood-results/dogfood-v0220-obsidian-vaults]]
- [[decision-log]]
