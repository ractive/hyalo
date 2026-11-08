---
type: iteration
title: "Iteration 338: an unresolved link is never a backlink"
date: 2026-11-08
status: completed
tags:
  - iteration
  - links
  - backlinks
branch: iter-338/backlinks-unresolved-links
related:
  - "[[decision-log]]"
  - "[[iterations/iteration-322-link-writers-agree]]"
---

`backlinks F` listed links that `find --fields links` reports as unresolved (`path: null`) whenever the link's raw
spelling equalled F's path. Measured on 85260414 (macOS): with `back\slash.md` in the vault and `[[back\slash]]`,
`[y](back\slash.md)` and `[[back%5Cslash]]` in `other.md`, `backlinks 'back\slash.md'` listed three links and only the
`%5C` one resolves; with `my%20file.md`, `[p](my%20file.md)` was listed as a backlink while `find` said `path: null`.

## Cause

`insert_file_links_with_options` (`link_graph.rs`) filed a `Resolution::Missing` link under its raw spelling
(`strip_site_prefix(target)`, or `normalize_target` for a relative markdown link, which keeps `\` as a filename byte off
Windows). `LinkGraph::backlinks` and `backlinks_ci` look a note up by its own path, with and without `.md`. The reader
resolves `\` as a separator and percent-decodes, so those spellings miss, but the raw key still equalled a real path.

## Decision (DEC-413)

A missing link is filed under `missing_key(spelling)`, `"\0missing:<spelling>"`, the way an ambiguous link already is
(`ambiguous_key`, DEC-384). A NUL byte cannot occur in a path, so no note can have that key. Resolution is unchanged and
there is no literal-backslash resolution (Obsidian refuses `\` in a file name); `link_target` is untouched.

## Audit of every reader of a graph key

Done before the change. The key change is a single arm of one function, but the audit missed one reader: a named note
that an ignore file excludes (first row), found in the PR #420 review.

| Reader | Where | Effect |
| --- | --- | --- |
| `backlinks FILE` for a note `.gitignore`, `.ignore` or global excludes keep out of discovery (DEC-342) | `commands/backlinks.rs` `run`, `prepared.rs` `refresh_index` | **Needed the raw key.** Its inbound links resolve to nothing in a whole-vault catalog and were listed only because the raw key equalled its path; without the key `backlinks` returned 0 while `find --file F --fields backlinks` returned 2. Fixed by folding the named note into the scan (disk) or the snapshot (`--index`) exactly as `find --file` does; pinned by `named_ignored_note_keeps_its_backlinks` |
| `backlinks` | `commands/backlinks.rs:93,95` | Stops listing an unresolved link (the fix) |
| `find --fields backlinks`, `--orphan`, `--dead-end` | `commands/find/mod.rs:1692` | Same; agrees with `backlinks` |
| `find --sort backlinks_count` | `commands/find/sort.rs:94,98,173,174` | Counts only resolved links |
| `summary` orphans and dead ends | `commands/summary.rs:432` | A note whose only inbound was an unresolved link is now an orphan |
| `summary` outbound side | `commands/summary.rs:416`, `LinkGraph::all_sources` | Unchanged: reads sources, and a note with only broken links stays an outbound source |
| `mv` single and batch | `link_rewrite.rs:206,220,224,1860,1862` | Unchanged: the graph picks the sources to open, then `catalog::resolve(..) == old_rel` gates every rewrite |
| `all_targets` | `link_graph.rs:281` | No caller outside tests |
| `rename_path` | `link_graph.rs:400` | No caller outside tests |
| `remove_source`, `total_edges` | `link_graph.rs:485,391` | Key-agnostic |
| `links fix`, `links auto` | `commands/links.rs:154` | Forward the graph, never read a key |
| HYALO006, `find --broken-links`, `summary.links.broken` | entries' links | Not the graph |
| Snapshot | `index.rs:2387` | The graph is re-resolved from the entries on read; a stale persisted graph is never authority, so no format bump (still 12) |

`mv` was measured, not argued: `mv --dry-run` of `back\slash.md` and `my%20file.md`, single and `--glob` batch, is
byte-identical on 85260414 and on this change (only `[[back%5Cslash]]` is rewritten).

The other readers of a single named, ignore-excluded note, measured against 85260414 for `.gitignore` and `.ignore`,
on disk and under `--index`: `find --file F` with `--fields backlinks`, `--orphan` and `--dead-end` is identical,
`summary` is identical, `mv --dry-run` of the note updates 0 links on both (its inbound links were never resolvable by
`mv`'s catalog), and `find --files-from` returns the note on disk and not under `--index` on both (DEC-301's batch
semantics). No other reader had the gap.
A snapshot written by one binary and read by the other answers as its reader does (checked both ways).

Tests that relied on the raw key, all graph-level: 15 unit tests in `link_graph.rs` and one in `commands/journal.rs`
queried `backlinks("target")` in a vault with no `target.md`; each now creates the note, which also makes their
negative assertions meaningful. `tests/e2e/backslash_filename.rs` linked `[[notes/back\slash]]`, one of the unresolved
spellings, and now links `[[notes/back%5Cslash]]`.

## Tasks

- [x] Branch `iter-338/backlinks-unresolved-links` from `origin/main`
- [x] Audit every reader of a graph key (table above)
- [x] `missing_key` in `link_graph.rs`; the Missing arm files under it
- [x] Core unit test `unresolved_link_is_not_a_backlink_of_the_note_it_spells`
- [x] E2E `iteration338_unresolved_backlinks.rs`: `[[back\slash]]` (unix), `[y](back\slash.md)` (unix), `[p](my%20file.md)`; `backlinks` and `find --fields backlinks` agree, on disk and under `--index`; `summary.orphans` follows; resolving links (`[[back%5Cslash]]`, `[[note]]`) still list
- [x] PR #420 review, finding 1: `backlinks` for a note that `.gitignore`/`.ignore` excludes resolves like `find --file --fields backlinks` (disk and `--index`); e2e `named_ignored_note_keeps_its_backlinks`; mutation-checked (named file not folded into the scan: fails on disk; not inserted into the snapshot: fails on the stderr note); the other named-file readers measured against main
- [x] PR #420 review, finding 2: DEC-413 and the audit table name the reader the audit missed
- [x] Mutation check: with `missing_key` returning the raw spelling, the unit test and all four new e2e tests fail
- [x] Fix the 16 existing tests that relied on a dangling target being a backlink
- [x] Corpus check (read-only `summary` and per-note backlinks): Obsidian Hub, GitHub Docs, MDN `files/en-us`
- [x] DEC-413, CHANGELOG, `.claude/CLAUDE.md`
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -q`
- [x] `hyalo lint` on the knowledgebase
- [x] Open the PR

## Results

- Mutation check: with the keying reverted the unit test and all four new e2e tests fail (the "resolving links still list" test on its `back\slash` leg, which also holds an unresolved `[[back\slash]]`); the five `backslash_filename` tests still pass, because their link now resolves.
- `cargo test --workspace`: 6 040 passed, 0 failed (1 145 CLI unit, 2 714 e2e, 1 817 core unit, 363 in the other targets), `cargo fmt` and `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Corpora (85260414 against this change, `summary --format json`, no `--index`): Obsidian Hub 6 519 notes, GitHub Docs 3 710, MDN 14 375: `summary` JSON byte-identical (`links.broken` 162 / 7 406 / 522, orphans 0 / 843 / 3 428, dead ends 191 / 369 / 851), every note's backlinks (source and line) identical, and `backlinks` JSON byte-identical for the five most-linked notes of each (re-run after the PR #420 fix). No real corpus has a note named by an unresolved spelling.
- Repro vault: `backlinks 'back\slash.md'` 3 links before, 1 after (`[[back%5Cslash]]`); `backlinks 'my%20file.md'` 1 before, 0 after; `summary` orphans 0 before, 1 after.
