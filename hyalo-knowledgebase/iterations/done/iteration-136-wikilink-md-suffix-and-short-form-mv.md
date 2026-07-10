---
title: >-
  Iteration 136 — Accept `.md` suffix in wikilinks; prefer short-form on `hyalo
  mv` rewrite
type: iteration
date: 2026-05-13
status: completed
branch: iter-136/wikilink-md-suffix-and-short-form-mv
tags:
  - iteration
  - bug-fix
  - links
  - mv
  - obsidian-compat
related:
  - "[[iterations/done/iteration-134-links-fix-short-form-wikilinks]]"
  - "[[iterations/done/iteration-135-batch-mv]]"
---

## Goal

Two related defects in hyalo's link handling, surfaced while dogfooding
`hyalo mv` against a real Obsidian vault:

1. **Resolver rejects `.md` suffix in wikilinks.** A wikilink like
   `[[iterations/done/iteration-42.md]]` resolves correctly in Obsidian
   (the `.md` suffix is allowed and ignored) but hyalo's own resolver
   flags it as broken. Round-tripping a vault through `hyalo mv` →
   `hyalo links fix` therefore produces spurious "broken" findings on
   links hyalo itself just wrote.
2. **`hyalo mv` over-qualifies link rewrites.** When a file moves, mv
   rewrites every referencing link to the new vault-absolute path
   (`[[iterations/done/iteration-42]]`), even when the basename is
   unique vault-wide and Obsidian's short-form (`[[iteration-42]]`)
   would resolve just fine. This violates the vault's convention
   (short-form for people/notes, established in iter-134) and creates
   path-form sprawl that breaks again the next time files move.

Fix both: teach the resolver to accept `.md` suffixes on wikilinks, and
teach `hyalo mv` to prefer short-form when the basename is unambiguous
across the vault, falling back to vault-absolute form only when needed
to disambiguate.

## Context

Iter-134 fixed `hyalo links fix` to recognize short-form wikilinks as
valid. This iteration is the symmetric fix on the **writer** side: when
hyalo emits a link (during `mv`), it should emit the same short form
the resolver and Obsidian both prefer, not a vault-absolute path.

The `.md`-suffix resolver gap is a separate but adjacent bug discovered
during the same dogfood pass — any wikilink that includes the file
extension (which Obsidian tolerates) is currently flagged. It's worth
fixing in the same iteration because both defects show up in the same
"after `hyalo mv`, run `hyalo links fix` and it complains" workflow.

## Scope

In scope:

- Wikilink parser/resolver in `crates/hyalo-core/src/links.rs` (and any
  call sites in `link_fix.rs` / `link_graph.rs` / `auto_link.rs`):
  treat a trailing `.md` on the target portion of a wikilink as
  optional. `[[foo]]`, `[[foo.md]]`, `[[path/foo]]`, `[[path/foo.md]]`
  all resolve to the same file.
- The case-insensitivity rules from iter-134 apply uniformly to both
  forms.
- `hyalo mv` rewriter (`link_rewrite.rs`): when rewriting a link that
  pointed at a moved file, choose the *minimal unambiguous form*:
  - If the basename (stem) of the new path is unique vault-wide
    (case-insensitively), emit `[[stem]]` (short-form).
  - Otherwise, emit vault-absolute path-form `[[new/path]]` (no `.md`).
  - Preserve the original link's form when it is already valid and
    unambiguous: short-form sources stay short-form, path-form sources
    stay path-form (unless the path-form would now be wrong).
- Markdown `[text](path.md)` links keep their path form (Obsidian and
  the spec both require it) — only wikilinks get short-form treatment.

Out of scope:

- Heading/anchor wikilinks (`[[foo#section]]`) — the `.md` suffix fix
  applies, but short-form policy stays as today.
- Aliased wikilinks (`[[foo|display]]`) — same: `.md` accepted, alias
  preserved verbatim.
- Embeds (`![[foo]]`) — same handling as plain wikilinks; in scope to
  the extent the resolver already covers them, no special-casing.
- Changing how `hyalo links auto` *emits* links (it already uses
  short-form by design).

## Tasks

- [x] Audit wikilink parsing in `links.rs` and locate the resolver
  entry point used by both `links fix` and `link_graph` (for `mv`).
- [x] Strip a single trailing `.md` from the target portion of a
  parsed wikilink before resolution. Document the rule in code
  comments and in `hyalo links fix --help` long-form text.
- [x] Add a `choose_link_form(new_path, vault_index, original_form)`
  helper that returns either `Short(stem)` or `Path(rel_path)` based
  on basename uniqueness in the vault and the original link form.
- [x] Wire `choose_link_form` into the `mv` rewriter so every emitted
  wikilink uses the chosen form. Markdown `[](.md)` links stay
  path-form.
- [x] Make sure relative-link rewrites inside the moved file follow
  the same policy (a wikilink from the moved file to a sibling that
  is still uniquely named becomes short-form, not a long relative
  path).
- [x] Update `hyalo mv --help` long-form text to describe the
  short-form preference and the disambiguation fallback.
- [x] Add e2e tests covering the matrix in "Test plan" below.
- [x] Dogfood: re-run the iter-135 batch-mv scenario against a real
  Obsidian vault, then `hyalo links fix` — expect zero broken /
  case-mismatch / ambiguous findings.

## Test plan

All tests are e2e under `crates/hyalo-cli/tests/` driving the
built `hyalo` binary against a tempdir vault.

### T1 — Resolver: `[[foo.md]]` resolves like `[[foo]]`

**Fixture.** `notes/foo.md` exists. `index.md` body:
`Plain [[foo]] vs suffix [[foo.md]] vs path [[notes/foo]] vs path+suffix [[notes/foo.md]].`

**Command.** `hyalo links fix`

**Assertions.** Zero broken, zero case-mismatch, zero ambiguous.
JSON envelope confirms all four wikilinks resolved to `notes/foo.md`.

### T2 — Resolver: case-insensitivity holds with `.md` suffix

**Fixture.** `notes/Foo.md` exists. `index.md` body: `[[FOO.MD]]`.

**Command.** `hyalo links fix` (on a case-insensitive-enabled platform).

**Assertions.** Resolves to `notes/Foo.md`; reported as a
`case_mismatch` (stem casing differs). `--apply` rewrites it to
`[[Foo]]` (short-form, **no** `.md`, and **no** path expansion —
consistent with iter-134).

### T3 — Heading wikilink with `.md` suffix

**Fixture.** `notes/foo.md` contains a heading `## Bar`. `index.md`
body: `See [[foo.md#Bar]].`

**Command.** `hyalo links fix`

**Assertions.** Zero broken; the heading anchor is preserved.

### T4 — Alias wikilink with `.md` suffix

**Fixture.** `notes/foo.md` exists. `index.md` body:
`See [[foo.md|the foo note]].`

**Command.** `hyalo links fix`

**Assertions.** Zero broken; alias text `the foo note` preserved
verbatim in any rewrite.

### T5 — `mv` prefers short-form for unique basenames

**Fixture.**

- `iterations/iteration-42.md` (unique basename vault-wide).
- `notes/index.md` body: `See [[iteration-42]] and [[iterations/iteration-42]].`

**Command.** `hyalo mv iterations/iteration-42.md --to iterations/done/iteration-42.md`

**Assertions.**

- `notes/index.md` body now reads:
  `See [[iteration-42]] and [[iteration-42]].`
- Both links use short-form because `iteration-42` is unique vault-wide.
- No occurrence of `[[iterations/done/iteration-42]]` anywhere.

### T6 — `mv` falls back to path-form for ambiguous basenames

**Fixture.**

- `a/dup.md` and `b/dup.md` both exist.
- `index.md` body: `See [[a/dup]].`

**Command.** `hyalo mv a/dup.md --to archive/dup.md`

**Assertions.**

- `dup` is no longer unique-after-move (still two `dup` files: `archive/dup.md`
  and `b/dup.md`), so `index.md` becomes `See [[archive/dup]].` — path-form,
  not short-form. **No** `.md` suffix.

### T7 — Single `mv` round-trip is idempotent under `links fix`

**Fixture.** Three iteration files, several `[[iteration-N]]` refs
across the vault, including frontmatter `related:` lists.

**Command sequence.**
1. `hyalo mv iterations/iteration-10.md --to iterations/done/iteration-10.md`
2. `hyalo links fix`

**Assertions.**

- Step 2 reports **zero** broken, case-mismatch, or ambiguous findings.
- No file is modified by step 2 (regression guard for the bug this
  iteration fixes — mv's output must not produce links its own
  resolver rejects).

### T8 — Batch `mv` (iter-135) output is also clean under `links fix`

**Fixture.** Five completed iteration files cross-linking each other.

**Command sequence.**
1. `hyalo mv --property status=completed --to iterations/done/ --apply`
2. `hyalo links fix`

**Assertions.**

- All five files end up in `iterations/done/`.
- All cross-references use short-form (basenames unique).
- Step 2 reports zero findings.

### T9 — Markdown link form unchanged

**Fixture.** `index.md` body:
`Markdown link: [foo](notes/foo.md). Wikilink: [[notes/foo]].`

**Command.** `hyalo mv notes/foo.md --to archive/foo.md`

**Assertions.**

- Markdown link becomes `[foo](archive/foo.md)` (path-form with `.md`,
  per spec).
- Wikilink becomes `[[foo]]` (short-form, no `.md`) since `foo` is
  unique vault-wide.

### T10 — Original path-form preserved when still unambiguous and explicit

**Fixture.** Two `note.md` files: `a/note.md`, `b/note.md`.
`index.md` body: `See [[a/note]].`

**Command.** `hyalo mv a/note.md --to a/renamed.md`

**Assertions.**

- Source author chose path-form (necessary then, because `note` was
  ambiguous). After mv, `renamed` is unique vault-wide → short-form
  becomes valid. Policy: when the user chose path-form *because they
  had to*, mv may switch to short-form once disambiguation is no
  longer needed. Link becomes `[[renamed]]`.
- Documented in `hyalo mv --help` so the behavior is not surprising.

## Acceptance criteria

- [x] `[[foo.md]]`, `[[path/foo.md]]`, `[[foo.md#heading]]`, and
  `[[foo.md|alias]]` all resolve identically to their `.md`-less
  counterparts.
- [x] `hyalo mv` rewriter emits short-form wikilinks when the new
  basename is unique vault-wide, vault-absolute path-form otherwise.
  Never emits `.md` suffix on wikilinks.
- [x] Round-trip `hyalo mv` → `hyalo links fix` produces zero
  findings and zero file changes on a vault that was clean before
  `mv`.
- [x] Markdown `[](.md)` links keep their spec-correct form.
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace -q` all pass.
- [x] Dogfood: against the user's Obsidian vault, the post-mv
  `hyalo links fix` reports zero false positives (regression
  acceptance for the original report).
