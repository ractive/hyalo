---
title: "Iteration 149 — hyalo new updates the snapshot index"
type: iteration
date: 2026-05-31
status: planned
branch: iter-149/new-updates-index
tags:
  - iteration
  - index
  - new
  - consistency
related:
  - "[[iterations/done/iteration-148-dogfood-v0160-iter147-fixes]]"
  - "[[dogfood-results/dogfood-v0160-iter-148-verify]]"
---

## Goal

Close the last mutating-command gap surfaced by the iter-148 index audit:
`hyalo new` creates a markdown file on disk but does **not** insert it into
the snapshot index. The new file is invisible to `find`, `summary`,
`properties`, `tags`, BM25 search, etc. until the next `create-index` —
surprising and inconsistent with every other mutator (`set`, `remove`,
`append`, `mv`, `task toggle/set`, `lint --fix`, `links fix/auto --apply`),
all of which keep the index in sync transparently.

## Scope decisions

### Add `mutation::add_index_entry`

The existing `mutation` module only has `update_index_entry` (no-op when the
file isn't already in the index), `rename_index_entry`, and
`save_index_if_dirty`. Adding a fresh file needs a new helper:

```rust
pub fn add_index_entry(
    snapshot_index: Option<&mut SnapshotIndex>,
    rel_path: &str,
    full_path: &Path,
    index_dirty: &mut bool,
) -> Result<()>
```

It should:

- Be a no-op when `snapshot_index` is `None` (no index loaded).
- Be a no-op when the entry already exists for `rel_path` (idempotent —
  callers shouldn't have to pre-check).
- Read the file fresh (parse frontmatter, body, tasks, sections, links) and
  insert a complete `IndexEntry` — same shape as a full-index-build pass
  would produce. Reuse whatever the index builder uses; don't re-implement.
- Set `*index_dirty = true` only when an entry was actually inserted.

### Wire `hyalo new` into the index

`crates/hyalo-cli/src/commands/new.rs::create_new()` currently ends after
`file.write_all(...)`. After a successful write (and outside `--dry-run`),
load the index if present, call `add_index_entry`, then
`save_index_if_dirty`. Same shape as `set.rs` lines 414–428.

If the index doesn't exist (no `.hyalo-index`), do nothing — `new` must
not silently create an index where the user hasn't asked for one.

### Edge cases to cover

- `--dry-run`: file isn't written → index isn't touched. (Same as today.)
- No index file present: no-op, no error.
- Index exists but `rel_path` somehow already in it (race / leftover):
  treat as `update_index_entry` — refresh from the new file's contents.
- Schema with no frontmatter fields ticked in: the entry is still inserted
  (other commands tolerate empty frontmatter; consistency wins).

## Tasks

- [ ] Add `mutation::add_index_entry` with the signature above; unit-test
      the four cases (no index, fresh insert, idempotent re-insert, dirty
      flag propagation)
- [ ] Wire `create_new` in `new.rs` to load index → `add_index_entry` →
      `save_index_if_dirty`, gated on non-dry-run + successful write
- [ ] E2E test: `hyalo create-index`, then `hyalo new --type X --file Y.md`,
      then `hyalo find --file Y.md` returns the new file from the index
      (verify by checking index was not rebuilt — file mtime preserved)
- [ ] E2E test: `hyalo new --dry-run` does not modify the index
- [ ] E2E test: `hyalo new` on a vault with no `.hyalo-index` succeeds
      and does not create one
- [ ] Update `hyalo new --help` if it mentions index behavior anywhere
- [ ] Update the rule-knowledgebase template / agent docs if they describe
      the index-update guarantees
- [ ] Mention the gap-closed in the iter-149 PR description

## Acceptance Criteria

- [ ] After `hyalo new --file foo.md`, `hyalo find --file foo.md` returns
      foo.md without a full index rebuild
- [ ] `hyalo new --dry-run` leaves the index byte-identical
- [ ] `hyalo new` is a no-op against the index when no `.hyalo-index` exists
- [ ] All existing tests pass; new unit + e2e tests added
- [ ] `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings &&
      cargo test --workspace -q` clean
