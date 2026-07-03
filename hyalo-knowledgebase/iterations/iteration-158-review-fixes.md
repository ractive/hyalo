---
title: Iteration 158 — Codebase review fixes (critical/high + key medium)
type: iteration
date: 2026-06-11
tags:
  - iteration
  - review
  - security
  - correctness
status: completed
branch: iter-158/review-fixes-2026-06
---

# Iteration 158 — Codebase review fixes

## Goal

Fix the critical and high findings (plus the cheap, same-root-cause medium items) from
the full-codebase review [[codebase-review-2026-06-11]]. The criticals are silent
data-loss bugs; the highs are a vault-escape, two memory-exhaustion paths, three
`lint --fix` corruption bugs, a non-atomic write that reverts a fix, and several
correctness/consistency defects. Pure-medium and low/nit findings are deferred to a
follow-up (see **Deferred** below).

## Context

The review ran as an 87-agent fan-out with adversarial verification (63 raw → 53
confirmed). Baseline gates were green. Findings cluster in three areas: the frontmatter
open-delimiter check disagrees between the read and write paths, `hyalo-mdlint`'s
line/column→byte conversion is wrong, and a few paths bypass the size cap / vault check /
atomic-write guarantees that the rest of the codebase already honours. Several fixes are
shared root causes — one change clears multiple findings.

## Critical

### C-1 — Frontmatter delimiter disagreement corrupts files (BOM + leading whitespace)

`crates/hyalo-core/src/frontmatter/parse.rs` uses three different opening-`---`
predicates: `extract_frontmatter` (`:517`, `starts_with("---")`),
`read_frontmatter_from_reader` (`:462`, `line.trim() == "---"`), and `find_body_offset`
(`:375`, trailing-only trim). When the read path accepts a delimiter the write path
rejects (a UTF-8 BOM, or a leading space before `---`), `write_frontmatter` prepends a
new frontmatter block on top of the original file and demotes the real frontmatter to
body — reported as success (exit 0). Affects `set`/`remove`/`append`.

- [x] Reconcile the three opening-delimiter predicates so the reader and `find_body_offset` always agree on where the frontmatter ends
- [x] Strip a leading UTF-8 BOM once at all three entry points (mirror `files_from.rs:43`)
- [x] Test: BOM-prefixed file round-trips through `set`/`remove`/`append` with no duplicate block and no data demoted to body
- [x] Test: leading-whitespace `---` either parses consistently or is rejected — never silently duplicated
- [x] Test: `remove --property X` on these files actually removes X (not leaves it in a buried block)
- [x] Extend the shared policy to the scanner and lint's body split (dogfood found `find` still blind to BOM frontmatter; `scanner/mod.rs` had two more hand-rolled opening checks, `lint.rs` `find_body_start` a third)

## High

### H-1 — `lint --fix` line/column→byte conversion is wrong (3 bugs, one fix)

`crates/hyalo-mdlint/src/engine.rs:474` advances `cur_col` by 1 per *char* while stock
rules + HYALO001 emit *byte* columns. Plus whole-file rules (MD047) are fed body-only
text. Consequences: MD009 injects a spurious blank line; autofix is silently dropped for
any non-ASCII line; MD047 never converges (reports `fixed=1` forever).

- [x] Make `line_col_to_byte` advance by `ch.len_utf8()` (treat columns as byte offsets)
- [x] Treat an end column of `line_len+1` as past-the-newline so an MD009 replacement that re-adds `\n` does not duplicate it
- [x] Run whole-file rules (MD047) against full file content, or adjust offsets for the body split
- [x] Test: MD009 fix removes trailing spaces without adding a blank line
- [x] Test: bare `[]` / trailing spaces on a line containing non-ASCII (e.g. `café`, CJK, emoji) are actually fixed
- [x] Test: MD047 converges to a single trailing newline and stops reporting a fix

### H-2 — `lint --fix` body write is non-atomic and reverts the frontmatter fix

`crates/hyalo-cli/src/commands/lint.rs:2183` uses `std::fs::write` (truncating, non-atomic)
and reconstructs the file from the *stale* pre-fix frontmatter slice captured at line
~1871, clobbering a frontmatter fix written earlier in the same run. No mtime guard exists
in `lint_one_file_extended`.

- [x] Reconstruct the body write from post-frontmatter-fix on-disk content (ideally a single combined frontmatter+body write)
- [x] Route the body write through `fs_util::atomic_write`
- [x] Add the `read_mtime` → `check_mtime` guard before the write (parity with `set`/`remove`/`append`/`task`)
- [x] Test: a file needing both a schema/frontmatter fix and a body fix ends with both applied

### H-3 — `hyalo mv` escapes the vault through a symlinked destination

`crates/hyalo-cli/src/commands/mv.rs:738` (and `:531` batch) never canonicalizes the
destination or calls `ensure_within_vault`; only literal `..`/absolute components are
rejected. A symlinked destination directory inside the vault relocates the file outside.

- [x] Canonicalize the destination's nearest existing ancestor and `ensure_within_vault` before any fs mutation, in `execute_mv` and `execute_batch_mv`
- [x] Test: `mv` into a symlink-that-points-outside is rejected (mirror `resolve_file_rejects_symlink_escape`)

### H-4 — Single-file `mv` leaves dangling frontmatter wikilinks

`crates/hyalo-core/src/link_rewrite.rs:184`: `plan_mv` calls `plan_inbound_rewrites`
(skips frontmatter) while the batch path uses `plan_inbound_rewrites_with_fm` (rewrites
frontmatter link properties). Single-file moves leave `related`/`depends-on`/etc.
pointing at the old path.

- [x] Make `plan_mv` rewrite frontmatter link-property wikilinks (call `plan_inbound_rewrites_with_fm` or fold the branch in)
- [x] Test: single-file `mv` rewrites inbound frontmatter wikilinks (mirror batch `t12_frontmatter_wikilink_rewrite`)

### H-5 — `lint` and `read` read whole files into memory with no size cap

`crates/hyalo-cli/src/commands/lint.rs:1872` (`read_to_string`, dispatched in parallel)
and `crates/hyalo-cli/src/commands/read.rs:160` (`Vec<String>`, no per-line cap) bypass
the scanner's 100 MiB `MAX_FILE_SIZE`. Measured 1.7–2.7 GiB RSS on large inputs.

- [x] `lint_one_file_extended`: stat-and-skip on `scanner::MAX_FILE_SIZE` before `read_to_string` (emit the scanner's stderr warning)
- [x] `read_body_lines`: stat-and-skip on `MAX_FILE_SIZE` and replace `reader.lines()` with `read_line_capped` (per-line `MAX_BODY_LINE_BYTES`)
- [x] Test: oversized file is skipped/refused by `lint` and `read` instead of OOMing

### H-6 — Error output is not JSON under `--format json`

`crates/hyalo-cli/src/dispatch.rs` returns `UserError(format!("Error: {e}"))` (raw text)
at ~18 sites (find: 474/481/489/496/508/514; mutation/lint/mv/links:
1121/1159/1194/1287/1295/1318/1831/1855/1861/1868/1874/1885). Under `--format json` (and
default piped mode) this emits plain text, breaking the documented `{"error": …}`
envelope. Sibling commands via `format_error` are already correct.

- [x] Route every `dispatch.rs` `UserError(format!("Error: …"))` site through `crate::output::format_error(format, …)`
- [x] Make `AppError::User`/`AppError::Exit` format-aware in `run.rs:338` instead of bare `eprintln!`
- [x] Test: `find --task '???'`, `set --where-property 'p~=/[/'`, `find --tag 'has space'`, etc. under `--format json` produce stderr parseable as JSON with an `.error` field

### H-7 — `--index` mutations corrupt the persisted link graph

`crates/hyalo-cli/src/commands/mutation.rs:32` `update_index_entry` patches only
`properties`/`tags`/`modified`; frontmatter link properties feed the link graph but
`entry.links` and the snapshot `LinkGraph` are never refreshed, so a persisted-wrong
backlink graph survives until a full `create-index`.

- [x] Route `update_index_entry` through `refresh_entry` (re-scan links) plus a link-graph update for the source path (mirror `rename_index_entry`)
- [x] Test: `set --file X --property related='[[foo]]' --index` then `backlinks foo --index` matches the live scan

### H-8 — BM25 ranking diverges between `--index` and default paths under a metadata filter

`crates/hyalo-cli/src/commands/find/mod.rs:450-465` builds a fresh BM25 corpus from only
the metadata-passing candidates (IDF over the filtered subset) while the persisted-index
path (`:318-341`) computes IDF over the full vault. Same query+filter ranks differently
depending on `--index`.

- [x] Seed the candidate-only corpus with full-vault corpus statistics (N, document frequencies, avgdl), or score over all scoped entries then intersect
- [x] Test: identical ranking for a body query + metadata filter with and without `--index`

### H-9 — `task toggle/set --line N` mutates checkbox lines inside code fences

`crates/hyalo-core/src/tasks.rs:514` (`toggle_tasks`) and `:561` (`set_tasks_status`)
resolve `--line` by raw indexing and validate only with `detect_task_checkbox`, ignoring
fence/comment state — unlike every other task path. Contradicts the documented contract
and the `task_read_inside_code_block_exits_1` guard on the read path.

- [x] Make `toggle_tasks`/`set_tasks_status` fence/comment-aware (resolve valid task lines via the scanner) and reject `--line` inside a fence/`%%` with the existing "line N is not a task" error
- [x] Test: `task toggle --line N` inside a code fence exits 1 (mirror the read-path test)

## Key medium (folded in — same root cause / cheap once nearby)

- [x] **CRLF frontmatter preserved**, not silently converted to LF (`crates/hyalo-core/src/frontmatter/parse.rs:285`); also covers MD009-on-CRLF (`engine.rs:460`) once H-1 lands
- [x] **`lint --fix` idempotency** (`crates/hyalo-cli/src/commands/lint.rs:2146`): re-lint after applying until a fixpoint (or guarantee single-pass convergence); mostly resolved by H-1 but verify a second pass changes nothing
- [x] **Fix conflict resolution severity tiebreak** (`lint.rs:2239`): an Error fix must win over an overlapping Warn fix on the same range
- [x] **`--dry-run` matches the real toggle for fenced `--line`** (`commands/tasks.rs:185` vs `:235`) — resolved by H-9; add a test asserting parity
- [x] **Schema lint group severity = max, not first violation** (`lint.rs:1741`): a genuine schema error must not be labelled `warn` because of ordering
- [x] **`read` text output control-byte sanitization** (`output_pipeline.rs:175`, `run.rs:561/571/599`): route the `RawOutput` body through `sanitize_control_chars` (extend iter-122)
- [x] **Size caps on remaining mutation paths**: `tasks.rs:437` and `frontmatter/parse.rs:262` (`write_frontmatter`) read whole files with no `MAX_FILE_SIZE` gate

## Deferred (follow-up iteration / backlog)

Not in scope for this iteration — captured here so they aren't lost:

- Snapshot index has no `format_version` (`index.rs:259`); validation failure invisible to scripts (`run.rs:1196`)
- Non-atomic `.hyalo.toml` writes (`lint_rules.rs:338,480`, `types.rs:578`, `views.rs:199`)
- Schema regexes without `size_limit` (`lint.rs:1004,1151`); MessagePack peak-memory amplification (`index.rs:502`)
- Maintainer absolute paths in 9 tracked KB files (info disclosure; re-runs iter-122 MED-2)
- `append` mapping error → wrong exit code (`append.rs:103`); `--sort score` / `=~` help-vs-behaviour mismatches; `--property title~=` help mismatch
- YAML comments destroyed on mutation (`parse.rs:269`) — document the limitation
- iter-157 stem-map behaviour change doc note (`dispatch.rs:62`)
- Rust nits: clone in `link_graph.rs:416`; duplicate intersection in `bm25.rs:693`; copy-pasted task dispatch (`dispatch.rs:911`); mdlint per-call allocations (`engine.rs:327`)

Discovered during this iteration's implementation:

- Oversized file inside a `--glob` mutation batch aborts the whole batch instead of per-file skip (set/remove/append treat the new `write_frontmatter` size error as fatal, not skippable)
- `plan_frontmatter_wikilink_rewrites` doesn't strip `#fragment` — anchored frontmatter wikilinks like `supersedes: "[[b#sec]]"` are not rewritten by `mv` in either single or batch mode (pre-existing, verified on both paths)
- `lint --fix` cosmetic: a fix that loses a pass-1 conflict but applies cleanly in pass 2 still counts in `total_conflicts` even though nothing remains unresolved (`fixed`/`remaining` counts are accurate)
- `rename_index_entry` (mv `--index` path) still uses `refresh_entry` for rewritten files, so their graph edges go stale — same class as H-7; `SnapshotIndex::refresh_links` is the ready building block
- Live-scan BM25 with a narrow metadata filter + body query now costs the same as an unfiltered query (correct global IDF requires reading the full scoped corpus) — documented in `create-index --help`; consider a runtime hint suggesting `--index` when filter selectivity is high

## Quality Gates

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace -q`

## Acceptance Criteria

- [x] C-1: BOM and leading-whitespace files survive `set`/`remove`/`append` with no duplicated frontmatter block and no data loss
- [x] H-1: MD009 adds no blank line; non-ASCII lines are fixed; MD047 converges
- [x] H-2: combined frontmatter+body `--fix` keeps both fixes; body write is atomic with an mtime guard
- [x] H-3: `mv` into a symlink escaping the vault is rejected
- [x] H-4: single-file `mv` rewrites frontmatter wikilinks
- [x] H-5: `lint` and `read` refuse/skip oversized files instead of exhausting memory
- [x] H-6: invalid-filter errors emit valid JSON under `--format json` across find/set/remove/append/lint/mv/links
- [x] H-7: `--index` mutation keeps the persisted link graph correct
- [x] H-8: BM25 ranking is identical with and without `--index` under a metadata filter
- [x] H-9: `task toggle/set --line` refuses lines inside code fences
- [x] Documentation (help text, README, knowledgebase) updated where behaviour changes
- [x] All quality gates pass
