---
title: "Iteration 157: Eliminate the per-invocation stem-map disk walk"
type: iteration
date: 2026-06-05
tags: [iteration, perf, dispatch, link-resolution]
status: planned
branch: iter-157/lazy-stem-map
---

# Iteration 157: Eliminate the per-invocation stem-map disk walk

## Motivation

Surfaced by [[dogfood-results/dogfood-v0160-firefox]] (finding F-1).

`crates/hyalo-cli/src/dispatch.rs:40-50` (`build_case_index_from_dir`)
walks the **entire** vault from disk on **every** CLI invocation to
populate a `CaseInsensitiveIndex` (used as both case-insensitive path
lookup and Obsidian short-form wikilink stem map). The doc-comment
acknowledges the cost ("the stem map is needed regardless of
case-insensitive path mode, and the index is cheap to build") — true
when the test corpora were a few hundred files, no longer true at
scale.

Measured on the firefox source tree (2621 `.md` files, 24 MB snapshot
index):

| Step                                        | Cost   |
|---------------------------------------------|-------:|
| process startup                             | 0.04 s |
| reading 24 MB index file (page-cache hot)   | 0.04 s |
| MessagePack decode (would be)               | <0.1 s |
| **`build_case_index_from_dir` (disk walk)** | **~2.7 s** |

`hyalo find --limit 1 --index-file fx.idx` on firefox takes 2.77 s
end-to-end, dominated by the unconditional stem-map walk. The snapshot
index is essentially free; the disk walk eats everything.

Most commands never resolve a wikilink. Inspection of all four
callsites of `maybe_case_index` in `dispatch.rs` (lines 524, 1015,
1344, 1854) shows the consumers are exactly `Find`, `Summary`,
`Links`, `Views`. And of those, `Find` only needs it when the user
passed a link-traversal flag (`--orphan`, `--broken-links`,
`--backlinks`, `--dead-end`). Every other command — `tags`,
`properties`, `task`, `read`, `set`, `remove`, `append`, `lint`,
`types`, `lint-rules`, `config`, `create-index`, `drop-index`,
`backlinks` (separate top-level), `outline`, `mv` (which builds its
own stem map inside `plan_mv` anyway) — pays the cost for nothing.

This gives us the first lever: a per-callsite `needs_stem_map: bool`
that gates the walk entirely for the ~70% of invocations that don't
need it (Part A below).

That still leaves the predicate-true commands — `find --orphan`,
`find --broken-links`, `summary`, `links`, etc. — paying the full
walk on every invocation, including when the user already loaded a
snapshot index. But the stem map is fully reconstructible from a list
of relative paths (the case-folded paths and stem→canonical maps are
both derivable from paths alone), and a snapshot index already
contains every entry's `rel_path`. So when the user passes
`--index-file`, we can seed the stem map from the index in
microseconds instead of re-walking the disk (Part B below).

Together, parts A and B close the F-1 cost driver completely for any
indexed vault and for the unindexed cases that don't need link
resolution.

### Why bundle A and B

They touch the same file and the same function (`maybe_case_index`),
the second wouldn't be measurable without the first (you can't tell
"walks happen sometimes" from "walks happen always" until you've
gated them), and the user benefit only fully materialises with both.
Splitting risks shipping half a fix and forgetting to follow up.

### Why **not** persist the stem map in the snapshot index

A third option — serialising the stem map's hash maps directly into
`.hyalo-index` — was considered and rejected: it offers the same
~microsecond savings as Part B's index-seeding (building from a path
list is essentially free), but requires a breaking index-format
change, a migration path for existing indices, and inflates the index
on disk. Part B gets the same speedup with strictly less complexity.

## Design

### Part A — Gate the walk by per-callsite predicate

Follow the established `needs_full_vault` / `needs_body` pattern at
`dispatch.rs:498-507` (`Commands::Find` arm). That code already
computes inline booleans from local flag state and threads them
through to the index resolver — the same shape applies here for the
stem map.

Extend `maybe_case_index` with `needs_stem_map: bool` and an
optional snapshot index reference (for Part B):

```rust
pub(crate) fn maybe_case_index(
    mode: CaseInsensitiveMode,
    dir: &std::path::Path,
    needs_stem_map: bool,
    snapshot: Option<&SnapshotIndex>,
) -> Option<CaseInsensitiveIndex> {
    if !needs_stem_map {
        // Empty index — link_resolve treats it the same as an empty
        // vault (lookups return None, callers degrade gracefully).
        return Some(CaseInsensitiveIndex::new());
    }
    let mut idx = if let Some(snap) = snapshot {
        build_case_index_from_snapshot(snap)   // Part B — µs
    } else {
        build_case_index_from_dir(dir)         // existing — disk walk
    };
    idx.set_case_insensitive_paths(mode_enabled(mode, dir));
    Some(idx)
}
```

### Part B — Seed from snapshot index when available

Add `build_case_index_from_snapshot(snap: &SnapshotIndex) ->
CaseInsensitiveIndex` next to the existing
`build_case_index_from_dir`. Body: walk the snapshot's entries and
insert each `rel_path` into a fresh `CaseInsensitiveIndex`, exactly
as the disk-walk variant does, but skip the file-system traversal:

```rust
pub(crate) fn build_case_index_from_snapshot(
    snap: &SnapshotIndex,
) -> CaseInsensitiveIndex {
    let mut idx = CaseInsensitiveIndex::new();
    for entry in snap.entries() {
        idx.insert(&entry.rel_path);
    }
    idx
}
```

Linear scan over ~thousands of entries; expected cost is
microseconds. The MessagePack deserialization of the snapshot index
itself is already paid by the caller in the path we care about
(commands that load `--index-file`), so this is pure additional
savings.

### Per-callsite computation

At each of the four call sites in `dispatch.rs`, compute the bool
inline from local flag state — *not* via a centralised predicate.
This mirrors how `needs_full_vault` and `needs_body` are computed for
`resolve_index` / `ScanOptions`:

| Callsite                | Computation                                            |
|-------------------------|--------------------------------------------------------|
| `Commands::Find` (524)  | `filters.orphan \|\| filters.broken_links \|\| !filters.backlinks.is_empty() \|\| filters.dead_end` |
| `Commands::Summary` (1015) | `true` (orphan / dead-end counts are always reported) |
| `Commands::Links` (1344)   | `true` (the entire command is about link resolution) |
| `Commands::Views` (1854)   | `true` (conservative — defer to underlying query)    |

The snapshot reference passed to `maybe_case_index` comes from the
same `IndexResolution::Resolved(ResolvedIndex::Snapshot(idx))` value
the callsite already extracted (or `None` for the
`ResolvedIndex::Scanned` branch — in which case the disk walk runs
as today, since we're already paying for a full disk scan anyway and
the stem map needs the same data).

The `Find` literal lives right next to the existing `needs_body` /
`needs_full_vault` computation, so the three booleans line up
visually and it's easy to keep them coherent when adding new flags.

At each of the four call sites in `dispatch.rs`, compute the bool
inline from local flag state — *not* via a centralised predicate.
This mirrors how `needs_full_vault` and `needs_body` are computed for
`resolve_index` / `ScanOptions`:

| Callsite                | Computation                                            |
|-------------------------|--------------------------------------------------------|
| `Commands::Find` (524)  | `filters.orphan \|\| filters.broken_links \|\| !filters.backlinks.is_empty() \|\| filters.dead_end` |
| `Commands::Summary` (1015) | `true` (orphan / dead-end counts are always reported) |
| `Commands::Links` (1344)   | `true` (the entire command is about link resolution) |
| `Commands::Views` (1854)   | `true` (conservative — defer to underlying query)    |

The `Find` literal lives right next to the existing `needs_body` /
`needs_full_vault` computation, so the three booleans line up
visually and it's easy to keep them coherent when adding new flags.

### Why per-callsite, not a centralised predicate

An earlier sketch proposed a `fn needs_stem_map(cmd: &Commands) ->
bool` exhaustively matched over `Commands` variants, called once at
dispatch entry. That sounds nice (compile-time guarantee that every
variant is considered) but it splits the `Find` flag-inspection logic
across two call sites: the predicate file *and* the existing
`needs_body` / `needs_full_vault` block on dispatch.rs:498-507. The
inline form keeps all three "what does this Find invocation need"
booleans together where the flags they read are already in scope.

Risk model: the failure mode if a future Find flag should-but-doesn't
flip the bool is that wikilink resolution silently misses. Same as
today when `discover_files` errors. Guardrail: the `needs_full_vault`
block is the natural review-time touch point for any new Find flag,
since reviewers already check it for index-scan implications; adding
`needs_stem_map` there puts the new logic on the same well-trodden
path.

### Conservative defaults

- `Views` is `true` even though a saved view may not need it. The
  alternative — inspect the resolved query post-expansion — costs
  more code than it saves on a command nobody invokes hot.
- `mv` is unchanged: it already constructs its own case index inside
  `plan_mv` at `link_rewrite.rs:121`, bypassing dispatch's
  `maybe_case_index` entirely. The plan_mv path is unaffected.

### Expected impact

Measured on the firefox tree (2621 files, 24 MB snapshot index):

| Command                          | Today  | After A only | After A + B |
|----------------------------------|-------:|-------------:|------------:|
| `find --limit 1`                 | 2.77 s | ~0.1 s       | ~0.1 s      |
| `find body "webkit" --limit 5`   | 2.81 s | ~0.1 s       | ~0.1 s      |
| `find --property title --limit 1`| 2.84 s | ~0.1 s       | ~0.1 s      |
| `tags`                           | ~2.7 s | ~0.1 s       | ~0.1 s      |
| `properties summary`             | ~2.7 s | ~0.1 s       | ~0.1 s      |
| `lint`                           | 3.45 s | ~1 s         | ~1 s        |
| `find --orphan --limit 5`        | 2.71 s | 2.71 s       | **~0.1 s**  |
| `find --broken-links`            | ~2.7 s | ~2.7 s       | **~0.1 s**  |
| `find --backlinks foo.md`        | ~2.7 s | ~2.7 s       | **~0.1 s**  |
| `summary`                        | 2.88 s | 2.88 s       | **~0.1 s**  |
| `links fix`                      | ~2.7 s | ~2.7 s       | **~0.1 s**  |

The right column applies only when `--index-file` is passed (or a
local `.hyalo-index` exists). Without an index, the predicate-true
commands still walk the disk as today — and that's fine, because
without an index we'd be doing a full disk scan for the query anyway.

Combined, the iteration eliminates the F-1 cost driver completely
for any vault with an index and for ~70% of unindexed invocations.

### Risk model

- **Part A — wrong `needs_stem_map` literal.** Failure mode: a
  command that should resolve wikilinks silently gets an empty stem
  map and link resolution misses. Same failure mode as today when
  `discover_files` errors (the doc-comment notes it "degrades
  gracefully to no case-insensitive fallback") — no new mode, just a
  slightly larger surface. Guardrail: per-flag unit tests over the
  `Find` arm, plus e2e tests for the predicate-true paths asserting
  byte-identical JSON output before and after.

- **Part B — stale snapshot index.** If a file was added to the
  vault since the snapshot was built, the seeded stem map won't know
  about it and wikilinks targeting it won't resolve until the user
  rebuilds the index. This is **identical to today's stale-index
  behaviour for every other query** — the snapshot is the contract.
  Users who care about freshness rebuild; users who don't pass
  `--index-file` go through the disk-walk branch and get current
  data. No new contract, just consistent application of the existing
  one. Document in the help text for `--index-file` if it isn't
  already.

## Non-goals

- **No on-disk index format change** (option #3 from the dogfood
  report). Seeding from the in-memory index entries gives the same
  speedup as persisting hash maps to disk, without the format
  migration, the size inflation, or the breaking change to existing
  `.hyalo-index` files. Explicitly rejected.
- No change to `mv`. It already builds its own case index inside
  `plan_mv`; that's a separate cost driver worth investigating but
  not in this iteration.
- No change to `create-index` cost. Building the snapshot index
  legitimately needs the full walk; this iteration only addresses the
  *redundant* walk on query-time invocations.
- No `--no-stem-map` flag. Per-callsite determinism beats a manual
  override; the user shouldn't have to think about it.
- No staleness-mtime check on the snapshot index for the seeded stem
  map. Out of scope — the snapshot contract already permits stale
  reads; applying it consistently here is the right behaviour. If
  users later want a "refuse stale" mode it can be a global setting,
  not specific to the stem map.

## Tasks

- [ ] **Part A:** Add `needs_stem_map: bool` parameter to
      `maybe_case_index` in `crates/hyalo-cli/src/dispatch.rs`. When
      `false`, return `Some(CaseInsensitiveIndex::new())` (empty
      index) without calling `build_case_index_from_dir`.
- [ ] **Part B:** Add `snapshot: Option<&SnapshotIndex>` parameter to
      `maybe_case_index`. When `needs_stem_map` is true AND a
      snapshot is provided, build the stem map from the snapshot's
      entries instead of walking disk. Add
      `build_case_index_from_snapshot(snap)` next to
      `build_case_index_from_dir` — body is a linear scan over
      `snap.entries()` inserting `entry.rel_path` into a fresh
      `CaseInsensitiveIndex`. When no snapshot is provided
      (`ResolvedIndex::Scanned` branch), keep the disk walk
      unchanged.
- [ ] Update the four call sites at `dispatch.rs:524, 1015, 1344,
      1854` to compute `needs_stem_map` inline from local flag state
      AND pass the snapshot reference from the surrounding
      `IndexResolution::Resolved(ResolvedIndex::Snapshot(idx))` (or
      `None` for the `Scanned` branch). Matches the per-callsite
      shape of the adjacent `needs_full_vault` / `needs_body` /
      `scan_body` computation:
      - **Find (524):** `needs_stem_map = filters.orphan ||
        filters.broken_links || !filters.backlinks.is_empty() ||
        filters.dead_end`. Place the binding directly next to the
        existing `needs_body` / `needs_full_vault` lines at 498-507
        so the three Find booleans cluster.
      - **Summary (1015):** `needs_stem_map = true`.
      - **Links (1344):** `needs_stem_map = true`.
      - **Views (1854):** `needs_stem_map = true`.
- [ ] Verify empty-index semantics: confirm none of the four
      consumers (`find_commands::find`, `summary`, `links`, `views`)
      crashes when handed an empty `CaseInsensitiveIndex`. Several
      already accept `Option<&CaseInsensitiveIndex>` and
      `link_rewrite.rs:593` defines `EMPTY_CASE_INDEX` as the
      established fallback — the empty `CaseInsensitiveIndex::new()`
      should behave identically.
- [ ] Unit test: parameterise the `Find`-flag computation as a free
      function (or inline closure with a test entry point) and assert
      one row per `(flag combination, expected bool)` pair —
      `--orphan`, `--broken-links`, `--backlinks X`, `--dead-end`,
      plain `find`, `find --property foo`, `find body-text`. Cheap
      regression guard for the inline literal.
- [ ] E2E test (Part A): on a synthetic large vault (≥1000 files,
      no wikilinks needed), `hyalo find --limit 1` completes under
      500 ms. Catches the Part A regression directly.
- [ ] E2E test (Part A): `find --orphan`, `find --broken-links`,
      `find --backlinks <file>`, `links fix`, `summary` orphan count
      produce byte-identical JSON envelopes before and after on a
      fixture vault that exercises wikilink resolution. These are the
      `needs_stem_map=true` paths; output must be unchanged.
- [ ] E2E test (Part A): a wikilink-using vault where `find`
      (without link-traversal flags) returns correct results even
      though the stem map is empty — i.e. the absence of stem-map
      data must not corrupt non-link queries.
- [ ] E2E test (Part B): on the same fixture vault (≥1000 files
      with wikilinks), `hyalo find --orphan --index-file <idx>` and
      `summary --index-file <idx>` both complete under 500 ms (vs
      the multi-second floor today). Output must match the disk-walk
      branch byte-for-byte.
- [ ] Unit test (Part B): `build_case_index_from_snapshot` on a
      handcrafted `SnapshotIndex` with a few entries returns the same
      `CaseInsensitiveIndex` (same stem map, same case-folded set)
      as `build_case_index_from_dir` on the corresponding on-disk
      vault. Anchors the equivalence claim.
- [ ] Update `dispatch.rs:29-66` doc-comments on
      `build_case_index_from_dir` and `maybe_case_index` to document
      the new `needs_stem_map` parameter and remove the "always
      returns Some" / "cheap to build" claims that are no longer
      accurate.
- [ ] Dogfood: re-run the firefox-tree timing matrix from
      [[dogfood-results/dogfood-v0160-firefox]] §F-1 and confirm the
      ~3 s → ~0.1 s drop on the predicate-false rows; record results
      in a follow-up dogfood note or update F-1 status.
- [ ] Run quality gates: `cargo fmt`,
      `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`.

## Acceptance criteria

- [ ] `hyalo find --limit 1 --index-file <idx>` on a vault of ≥2000
      files completes in under 500 ms on a developer machine (was
      ~2.7 s on firefox tree before).
- [ ] `hyalo find --orphan --index-file <idx>` and
      `hyalo summary --index-file <idx>` on the same vault also
      complete in under 500 ms (Part B). Without `--index-file`,
      these commands still take their disk-walk-bound time — which
      is acceptable because without an index they're already paying
      for a full disk scan anyway.
- [ ] `hyalo find --orphan`, `find --broken-links`,
      `find --backlinks <file>`, `links fix`, and `summary` all
      produce byte-identical JSON envelopes before and after on a
      fixture vault that exercises wikilink resolution. (Verified by
      e2e diff test.) Same JSON whether the stem map came from disk
      walk or snapshot seed.
- [ ] Each of the four `maybe_case_index` callsites in `dispatch.rs`
      computes `needs_stem_map` inline from local flag state and
      passes the snapshot reference when available, matching the
      shape of the existing `needs_full_vault` / `needs_body`
      bindings. No centralised predicate; the decision lives next to
      the flags that drive it (matches the codebase's established
      "needs_full_vault" pattern at `dispatch.rs:498-507`).
- [ ] No new public API surface; `build_case_index_from_dir`,
      `build_case_index_from_snapshot`, and `maybe_case_index` all
      remain `pub(crate)`.
- [ ] No `.hyalo-index` format change. Existing indices work without
      rebuild.
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, and `cargo test --workspace -q` all pass.

## Open questions

- Should `Views` be smarter — inspect the resolved underlying query
  and decide based on that, rather than the conservative `true`?
  Defer: low value, more code, easy to revisit if profiling later
  shows view dispatch is hot.
- Should this be combined with a future iteration that lazy-builds
  the stem map *on first use* inside the four predicate-true
  consumers? After Part A + B, the only remaining stem-map cost is
  on predicate-true paths *without* `--index-file`, and those paths
  are already paying for a full disk scan. Lazy-on-first-use would
  only win when the predicate-true consumer never actually queries
  the stem map (e.g. `find --orphan` on a vault with zero orphans).
  Probably not worth a follow-up; flag if profiling later shows
  otherwise.
- Should `mv` route through the same dispatch-level case-index value
  rather than rebuilding inside `plan_mv`? Out of scope; addresses a
  separate but related cost driver. File as follow-up if measurement
  warrants.
