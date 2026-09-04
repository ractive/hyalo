---
title: Stale-index check matrix before DEC-280
type: research
date: 2026-09-04
tags:
  - research
  - index
  - iteration-265
status: completed
related:
  - "[[iterations/iteration-265-scan-exclude-and-skipped-files]]"
  - "[[decision-log]]"
---

# Stale-index check matrix before DEC-280

The dogfood report's UX-7 said the `.hyalo-index` staleness check "exists on
some commands and not others". This note is the enumeration that finding asked
for — what each `--index` path actually did before iteration 265 — and the
reasoning that turned it into one policy ([[decision-log|DEC-280]]).

## What each path did

| Path | Check | Cost | Effect on a stale entry |
| --- | --- | --- | --- |
| Index load (all `--index` commands), `run.rs` | Bounded-depth walk of the vault's **directory** mtimes vs the snapshot's `created_at`, with `STALENESS_TOLERANCE_SECS` slack (iter-249) | One bounded directory walk per run | `warning: index older than vault; results may be stale — re-run create-index`, then serves the snapshot unchanged (iter-247 S-2: warn-but-serve) |
| `links auto` / `links fix` `--index`, `commands/links.rs` | `files_modified_since_snapshot` (per-entry mtime) **and** `files_missing_from_snapshot` (full `discover_files` walk) | One `stat` per indexed entry plus a directory walk | Refreshes the entry through `MutationJournal`, persisting only on `--apply`; warns naming the first offender |
| `set` / `remove` / `append` / `task` `--index`, `commands/set.rs` | `journal.refresh_if_stale(rel, full, mtime)` on the files it is about to write | One `stat` per target — free, the write path already had it | Refreshes silently, then applies the mutation |
| `find` / `read` / `backlinks` / `lint` `--index` | Nothing beyond the shared load-time probe | — | Answered from the snapshot |

## The gap that mattered

Only the last row is wrong in a way a user notices. `find --index --file
just-appended.md` named exactly one file, and answered with that file's
pre-append `size` and `lines` while emitting a warning about the *whole* index
that a caller reading `.results[0].lines` never sees. The mutating commands
already proved the fix is cheap: they stat the files they are about to touch.

The directory-mtime probe is also strictly weaker than a per-file stat. It
cannot see an in-place edit that leaves the directory mtime alone, and it fires
on an unrelated directory touch — so on a busy vault it is simultaneously
noisy and unreliable. It survives only because it is O(1) in the number of
entries, which is what a whole-vault query needs.

## The policy

DEC-280: refresh what the run named; warn only when it named nothing.

- A `--index` run with explicit targets (`--file`, a positional path, and no
  `--glob`) stat-refreshes exactly those entries — mtime **and** size, since an
  append inside the same second moves only the length — and stays silent.
- A run with no explicit targets keeps the directory-mtime probe and its
  warning.

Rejected: making a full-vault refresh implicit on every `--index` read. That is
the cost [[iterations/iteration-260-lazy-bm25-snapshot-load]] removed (396 ms →
151 ms on MDN), paid by every query, to fix a case that only arises when the
run named its files. Per-file stat is O(targets); a full refresh is O(vault).

Also rejected: turning staleness into a hard refusal. The probe is a heuristic
and mtime granularity varies by filesystem, so a refusal would make indexed
queries fail for reasons the user cannot act on (iter-247, S-2).

## Where

- `crates/hyalo-core/src/index.rs` — `refresh_if_changed_on_disk`,
  `files_modified_since_snapshot`, `files_missing_from_snapshot`,
  `newest_dir_mtime`, `STALENESS_TOLERANCE_SECS`
- `crates/hyalo-cli/src/mutation.rs` — `Commands::explicit_file_targets`
- `crates/hyalo-cli/src/run.rs` — the load-time probe and the targeted refresh
