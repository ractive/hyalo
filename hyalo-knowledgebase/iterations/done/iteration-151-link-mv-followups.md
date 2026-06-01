---
title: >-
  Iteration 151 — Link/mv follow-ups (self-link, form preservation, ambiguity
  diagnostic)
type: iteration
date: 2026-06-01
status: completed
branch: iter-151/link-mv-followups
tags:
  - iteration
  - links
  - wikilinks
  - mv
  - tests
related:
  - "[[dogfood-results/dogfood-v0160-iter-150-crazy]]"
  - "[[iterations/done/iteration-150-link-handling-refactor]]"
  - "[[iterations/done/iteration-136-wikilink-md-suffix-and-short-form-mv]]"
  - "[[iterations/done/iteration-137-cross-platform-link-resolution]]"
---

## Goal

Close the three follow-ups from
[[dogfood-results/dogfood-v0160-iter-150-crazy]] that fell out of the
iter-150 refactor. None require new architecture — the `LinkResolver`
and `LinkWriter` from iter-150 are the right substrate; the gaps are
all in the *callers* and *diagnostics* layer.

1. **NEW-1 (HIGH)** — self-referencing links inside a `mv`'d file are
   not rewritten. `mv x.md y.md` leaves `[[x]]` inside the moved file
   pointing at the now-nonexistent `x.md`.
2. **NEW-2 (MEDIUM)** — `[[./b]]` and `[[b.md]]` forms round-trip on
   sibling moves but collapse to bare path-form on cross-directory
   moves. The `WrittenForm` is detected at parse time but lost during
   re-emission when the linker and source live in different
   directories.
3. **NEW-3 (MEDIUM)** — `mv` correctly *refuses* to rewrite ambiguous
   inbound links, but emits no diagnostic. The user must run
   `hyalo links` separately to discover anything was skipped.

Plus: lock the link-handling behavior down with a comprehensive,
table-driven test matrix so the next dogfood round cannot surface an
*eighth* shape of the same family.

## Why now

iter-150 closed BUG-1/BUG-2 from iter-149 and unified the resolver +
writer. The dogfood pass found three residual gaps, all in caller code
or output paths, none requiring further architecture work. This is the
right moment to lock the contract with tests before the link surface
sees its next feature request.

The user explicitly asked for test-heavy coverage on link handling in
this iteration ("Make sure to spend some time to write tests for the
link handling").

## Scope

### IN — bug fixes

**1. Self-link rewrite on `mv` (NEW-1).**

`commands/mv.rs::run` currently iterates `plan_inbound_rewrites` over
inbound files (files that link *to* the moving file). The moving file
itself is excluded. Fix: after the rename, run the same rewrite plan
*on the destination file*, treating any link whose resolved target
was the *old* source path as inbound. Concretely:

- Before the rename, collect the set of (span, resolved_target) tuples
  inside the source file where `resolved_target == source_path`.
- After the rename, splice those spans in the destination file using
  `LinkWriter::rewrite(span, new_path, PreserveForm)`.
- Count them in `total_files_updated` / `total_links_updated` like any
  other inbound rewrite.

Self-links via every shape must work: `[[x]]`, `[[./x]]`, `[[x.md]]`,
`[[x|alias]]`, `[[x#sec|alias]]`, markdown `[a](x.md)`. The
`PreserveForm` policy already handles each.

**2. Cross-directory form preservation (NEW-2).**

When a link `[[./bulk/file-1]]` in `/linker.md` is rewritten after
`mv bulk/file-1.md bulk/moved-1.md`, the new target is
`bulk/moved-1.md` relative to the vault root. The current writer
detects `WrittenForm::DotRelative` on parse and stores it, but the
re-emission code only re-applies the `./` prefix when the *segment
count* of the new target matches the old. Cross-dir moves where only
the basename changes within the same subdir end up dropping `./` and
`.md` because the writer falls back to `PathRelative` emission.

Fix in `link_write.rs::LinkWriter::emit_target`:

- For `DotRelative`: if the new target is reachable from the linker
  via `./<segments>`, emit `./<segments>`. Concretely, if
  `new_target.starts_with(linker_dir)` (after vault-relative
  normalisation), emit `./` + the tail. The current code only checks
  this for the same-directory case.
- For `MdSuffixed`: always re-append `.md` to the emitted target,
  regardless of whether the resolver would have found the target
  without it. The suffix is a stylistic choice, not a resolution
  hint, after the iter-136 work.

Important: form-preservation must compose with frag + alias
preservation. The existing tests for those should not regress.

**3. mv-time ambiguity diagnostic (NEW-3).**

`commands/mv.rs` currently swallows `Resolution::Ambiguous` from
`LinkResolver::resolve` and skips the rewrite without surfacing it to
the user.

Fix: extend the `mv` envelope JSON with:

```json
{
  "skipped_ambiguous": [
    {
      "source": "linker.md",
      "line": 6,
      "target": "target",
      "candidates": ["a/target.md", "b/target.md"]
    }
  ]
}
```

And on text-format output, emit one stderr line per skipped link:

```
note: skipped ambiguous link [[target]] at linker.md:6
      candidates: a/target.md, b/target.md
      (use --allow-ambiguous to rewrite based on stem match anyway)
```

Audit `--allow-ambiguous`: the flag exists today (clap arg in
`commands/mv.rs`) but the dogfood repro showed no behavioral
difference. Determine whether it's wired through to the resolver
call-site and either fix the wire-up or remove the flag (and document
the removal in the PR). Either outcome is fine — but `--help` and
behavior must match.

### IN — test hardening (the user-requested focus)

Add `crates/hyalo-cli/tests/e2e/mv_link_forms.rs` as a single
table-driven test module. The matrix:

- **Shapes** (8): `[[b]]`, `[[./b]]`, `[[b.md]]`, `[[b|alias]]`,
  `[[b#sec]]`, `[[b#sec|alias]]`, `[[./b#sec|alias]]`, `[[b.md|alias]]`.
- **Topologies** (4): sibling-same-dir, cross-dir (linker root,
  target nested), cross-dir-deep (linker nested, target other-nested),
  same-dir-rename-only.
- **Move kinds** (3): rename-in-place, move-down (a → sub/a),
  move-up (sub/a → a).
- **Selflink** (boolean): include a self-reference shape in the
  moving file's own body.

Generate a fixture vault per combination, run `hyalo mv`, assert the
post-mv link text byte-equal to the expected form, assert the
post-mv `hyalo links` output reports `broken: 0` and `ambiguous: 0`.

Plus the dedicated bug-repro tests (each test named after the bug):

- `bug_iter150_new1_selflink_basic` — `[[x]]` in `x.md` after mv.
- `bug_iter150_new1_selflink_with_alias` — `[[x|me]]` in `x.md`.
- `bug_iter150_new1_selflink_dot_relative` — `[[./x]]` in `x.md`.
- `bug_iter150_new1_selflink_md_suffix` — `[[x.md]]` in `x.md`.
- `bug_iter150_new1_selflink_markdown_form` — `[a](x.md)` in `x.md`.
- `bug_iter150_new2_dot_relative_cross_dir` — `[[./bulk/f]]` survives
  with `./` after `mv bulk/f.md bulk/g.md`.
- `bug_iter150_new2_md_suffix_cross_dir` — `[[bulk/f.md]]` survives
  with `.md`.
- `bug_iter150_new3_ambiguous_emits_diagnostic` — assert
  `skipped_ambiguous` JSON array populated with source/line/candidates.
- `bug_iter150_new3_ambiguous_text_stderr` — assert stderr contains
  the `note: skipped ambiguous link` line.
- `bug_iter150_new3_allow_ambiguous_behavior` — either asserts the
  flag does something specific (chosen-arbitrarily rewrite + warning)
  or asserts the flag is gone from `--help` (whichever path the
  implementation takes).

Plus property-style tests for `LinkWriter` (in
`crates/hyalo-core/src/link_write.rs`):

- For every `(WrittenForm, linker_dir, target_dir, new_target_dir,
  basename)` combination, the writer emits a string that, when
  re-parsed by `LinkResolver`, resolves to the new target. This is
  the round-trip invariant.

### IN — small UX/docs

- `mv --help` mentions self-links are rewritten too.
- `mv --help` documents `--allow-ambiguous` (or removes it; see
  above).
- README link-handling section updated if it claims any of these
  behaviors today.

### OUT (deferred)

- **`links fix` / `links auto` migration to `LinkWriter`** — still
  pending from iter-150; orthogonal to this iteration's bugs. Track
  separately if needed.
- **`mv`-side `.hyalo-index` incremental patch** — covered by
  [[iterations/iteration-154-mv-index-patch]].
- **Reference-style markdown links** — not in scope.
- **Image links** — not in scope.
- **Property tests under `proptest`/`quickcheck`** — overkill for
  this round; the explicit matrix is enough.

## Tasks

- [ ] NEW-1: Collect self-link spans in source before rename in
      `commands/mv.rs::run`
- [ ] NEW-1: Apply `LinkWriter::rewrite` on destination file post-rename
- [ ] NEW-1: Count self-link rewrites in mv envelope totals
- [ ] NEW-1: Verify every shape (`[[x]]`, `[[./x]]`, `[[x.md]]`,
      `[[x|a]]`, `[[x#s|a]]`, `[a](x.md)`) round-trips on self
- [ ] NEW-2: Fix `DotRelative` re-emission in
      `link_write.rs::emit_target` for cross-dir descendant targets
- [ ] NEW-2: Fix `MdSuffixed` re-emission to always re-append `.md`
- [ ] NEW-2: Confirm composition with `#frag` + `|alias` unchanged
- [ ] NEW-3: Propagate `Resolution::Ambiguous` from `LinkResolver`
      through `plan_inbound_rewrites` to `commands/mv.rs`
- [ ] NEW-3: Add `skipped_ambiguous` array to mv JSON envelope
- [ ] NEW-3: Emit `note: skipped ambiguous link ...` on stderr in
      text format
- [ ] NEW-3: Audit `--allow-ambiguous`; wire it through or remove it
- [ ] Tests: add `tests/e2e/mv_link_forms.rs` with the 8 × 4 × 3 × 2
      matrix
- [ ] Tests: add 10 named bug-repro tests (see list above)
- [ ] Tests: add `LinkWriter` round-trip unit tests in
      `crates/hyalo-core/src/link_write.rs` covering every
      `WrittenForm` × topology combination
- [ ] Tests: add a `mv` test that runs the dogfood NEW-1 repro
      verbatim (`x.md` self-link → `y.md`) and asserts `broken: 0`
- [ ] Docs: `mv --help` mentions self-link handling
- [ ] Docs: `mv --help` documents or removes `--allow-ambiguous`
- [ ] Docs: README link-handling section
- [ ] `cargo fmt && cargo clippy --workspace --all-targets -- -D
      warnings && cargo test --workspace -q` clean
- [ ] Cross-platform CI green (Linux + macOS + Windows)
- [ ] Mark `status=completed`, move plan to `iterations/done/`

## Acceptance Criteria

- [ ] **NEW-1 closed.** `mv x.md --to y.md` on a file containing any
      self-link shape produces a destination file whose self-links
      point to `y` (form-preserved), and `hyalo links` reports
      `broken: 0` immediately after.
- [ ] **NEW-2 closed.** `[[./bulk/f]]` survives as `[[./bulk/g]]`
      after `mv bulk/f.md bulk/g.md`; `[[bulk/f.md]]` survives as
      `[[bulk/g.md]]`. Sibling-case round-trips unchanged.
- [ ] **NEW-3 closed.** mv against a vault with an ambiguous inbound
      link produces a `skipped_ambiguous` array (JSON) and a stderr
      `note:` line (text), each citing source file, line, target,
      and candidates.
- [ ] **`--allow-ambiguous` is coherent.** Either the flag has a
      tested effect and is documented, or the flag is removed and the
      removal noted in the PR.
- [ ] **Test matrix lands.** `tests/e2e/mv_link_forms.rs` contains
      the full 8 × 4 × 3 × 2 = 192-case matrix, all green, run on
      all three OSes in CI.
- [ ] **Bug-repro tests are named after the bugs.** Future dogfood
      reports can grep `bug_iter150_new1` and find the regression
      gate.
- [ ] **No regression.** Every existing link-related e2e test passes
      unchanged.
- [ ] `cargo fmt && cargo clippy --workspace --all-targets -- -D
      warnings && cargo test --workspace -q` clean.

## Notes for the implementing agent

- Read [[dogfood-results/dogfood-v0160-iter-150-crazy]] for exact
  reproductions of NEW-1/2/3 before touching code.
- The self-link fix (NEW-1) is the highest user-visible win. Land it
  in a separate commit so the test diff is reviewable on its own.
- For NEW-2, the temptation will be to over-engineer
  `WrittenForm`-driven emission. Resist — the writer already has the
  form; just consult it consistently. The bug is a missed branch,
  not a missing concept.
- The test matrix is large but mechanical. A single helper
  `assert_mv_preserves(linker, target, new_target, written, expected)`
  plus a parameterised loop keeps each case to one line of fixture.
- Cross-platform CI is non-optional. The iter-137 retrospective
  applies: run on Linux locally (in a container) before requesting
  review.
- Keep `links fix` / `links auto` out of scope. Those still use the
  pre-iter-150 paths and are a separate iteration.
