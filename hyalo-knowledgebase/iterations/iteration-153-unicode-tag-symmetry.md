---
title: Iteration 153 — Unicode tag write/query symmetry
type: iteration
date: 2026-06-01
status: completed
branch: iter-153/unicode-tag-symmetry
tags:
  - iteration
  - tags
  - unicode
  - bug
related:
  - "[[dogfood-results/dogfood-v0160-iter-149-creative]]"
  - "[[dogfood-results/dogfood-v0160-iter-150-crazy]]"
---

## Goal

Close BUG-4 from
[[dogfood-results/dogfood-v0160-iter-149-creative]]: the tag write path
accepts unicode tags (`日本語`, `emoji-🎉`, etc.) and indexes them, but
the query path rejects them with `invalid character '日' in tag name`.
A tag can be stored but never searched.

## Why now

Asymmetric write/query semantics are a contract violation. Either the
storage path is wrong (it shouldn't accept these) or the query path is
wrong (it shouldn't reject them). Today the only escape hatch is
hand-editing frontmatter, which defeats the point of `hyalo set
--tag`. Severity MEDIUM, but easy to fix.

## Scope

### IN — pick one side and commit

**Option A (recommended): broaden query to match write.**

Tags are a user-facing label. Real KBs in non-Latin languages need
unicode tags (the dogfood vault has them today). The query path's
character class restriction (`letters, digits, _, -, /`) appears to be
ASCII-centric — broaden to Unicode `alphanumeric` per `char::
is_alphanumeric` (which covers CJK, Cyrillic, Greek, etc.), plus
`_`, `-`, `/`, and a curated subset of common symbols if needed.

**Option B: tighten write to match query.**

Reject `日本語` at write time. Update `hyalo set --tag`, `hyalo new
--tag`, and the YAML reader's tag-index path. Existing files with
unicode tags become non-indexable; lint surfaces them as warnings.

Recommendation: **A**. The web's tag conventions (Mastodon, Bluesky,
GitHub labels) all accept unicode. Restricting hyalo to ASCII tags
would be surprising for non-English KBs.

### IN — both options, common work

- Define a single `is_valid_tag_char(c: char) -> bool` in
  `crates/hyalo-core/src/tags.rs` (or wherever the current validator
  lives) and call it from both write and query paths. The current
  drift exists because they live in separate functions.
- Document the rule in `--help` text for `find --tag`, `set --tag`,
  `new --tag`.
- Document the rule in the README tag section.

### IN — emoji handling

Emoji are technically not `is_alphanumeric`. Decide explicitly:

- **Allow** emoji as part of the alphanumeric-extended set (matches
  what the write path currently does — `emoji-🎉` is accepted today).
- **Reject** emoji uniformly on both sides.

Recommendation: **allow**. Same rationale as option A. If a user
writes a tag with an emoji, the only sane behaviour is to let them
search for it. Update the validator to permit Unicode alphabetic
characters *and* emoji (use `unicode-segmentation` or check `c.
is_alphabetic() || c.is_numeric() || matches!(c, '_' | '-' | '/') ||
is_emoji(c)`).

### OUT

- Tag normalisation (case-folding, NFC). Out of scope; tag identity
  is currently codepoint-equal and that's fine.
- Tag hierarchy semantics (the `/` separator).
- Property-value character classes. This iteration is tags-only.

## Tasks

- [x] Decide A vs B (recommend A), note in PR
- [x] Define single `is_valid_tag_char` in `tags.rs`
- [x] Call from `set --tag`, `new --tag`, `find --tag`,
      tag-index population, `hyalo tags` listing
- [x] Decide emoji policy (recommend allow), implement
- [x] Update `find --tag` `--help` text
- [x] Update `set` / `new` `--help` text
- [x] Update README tag section
- [x] Test: `bug_iter149_4_unicode_tag_roundtrip` — set tag `日本語`,
      then `find --tag 日本語` returns the file
- [x] Test: `bug_iter149_4_emoji_tag_roundtrip` — set tag
      `emoji-🎉`, then `find --tag emoji-🎉` returns the file
- [x] Test: `bug_iter149_4_invalid_tag_chars_still_rejected` — `set
      --tag "foo bar"` (space) still rejected on both sides
- [x] Test: round-trip with NFC vs NFD normalised input behaves
      consistently (or document the limitation)
- [x] `cargo fmt && cargo clippy --workspace --all-targets -- -D
      warnings && cargo test --workspace -q` clean
- [x] Mark `status=completed`, move to `iterations/done/`

## Acceptance Criteria

- [x] Tag validator lives in one place, called by every read and
      write path.
- [x] `set --tag 日本語 f.md && find --tag 日本語` returns `f.md`.
- [x] `set --tag 'emoji-🎉' f.md && find --tag 'emoji-🎉'` returns
      `f.md`.
- [x] Invalid characters (space, `#`, etc.) are rejected at *both*
      `set` and `find`, with identical error messages.
- [x] `--help` and README document the rule.
- [x] No regression on existing ASCII tag tests.

## Notes for the implementing agent

- Repro from [[dogfood-results/dogfood-v0160-iter-149-creative]]
  BUG-4 — copy into tests verbatim.
- One validator function, two callers. The temptation is to write a
  small parser/grammar; resist — `char::is_alphanumeric` plus a
  handful of explicit allowed punctuation is all this needs.
- Don't conflate this with tag *normalisation* (case-folding,
  NFC/NFD). That's a different iteration if anyone ever asks for it.
