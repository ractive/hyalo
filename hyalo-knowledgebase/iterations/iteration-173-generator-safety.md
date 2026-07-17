---
title: >-
  Iteration 173 — generator safety (non-destructive adopt, skip-and-warn, case,
  clean output)
type: iteration
date: 2026-07-17
tags:
  - iteration
  - okf
  - generators
  - fix-wave
status: completed
branch: iter-173/generator-safety
---

# Iteration 173 — generator safety & consistency

## Goal

No hyalo generator may ever destroy hand-written content or die on a file it
didn't need: `okf index --apply` adopts marker-less files preserving every
byte, generators skip-and-warn on malformed files, reserved-file matching
works on case-insensitive filesystems, generated markdown passes hyalo's own
lint, and the generators' `--format text` output is readable. Fixes release
blocker **RB-2** and the generator half of **RB-3** from
[[dogfood-results/dogfood-v0180-okf-profiles-pre-release]].

## Decisions (taken 2026-07-17, do not re-litigate — see DEC-052)

- **Auto-adopt, preserve all**: first apply on a marker-less `index.md`
  inserts the managed region while keeping the existing body; destructive
  replacement only via an explicit `--replace` flag.
- **Case handling reuses the `[links] case_insensitive` auto approach**
  (FS-detected), rather than a new knob.

## Note from iter-172

No scope overlap with iter-172's merge-engine/bind-typing work (this
iteration doesn't touch `profiles.rs`, `.hyalo.toml` merge, or
`[schema.bind]`). One process lesson carries forward: the `ac-fidelity-check`
gate requires each ticked Acceptance Criteria bullet to be a single line
containing a backtick-quoted symbol (test fn name or code identifier) that
appears in the diff — multi-line prose bullets with no quoted symbol fail the
gate even when a real test backs the claim. When closing out this iteration,
write ACs as one-liners naming the backing test/symbol up front rather than
adding it after the fact.

## Tasks

### 1. Non-destructive adoption (RB-2, data loss) [4/4]

- [x] `okf index --apply` on an existing `index.md` WITHOUT okf markers:
  preserve the entire existing body and append the managed region (markers +
  generated content) after it; never drop content (mapl repro: H1 + prose +
  manual list must all survive)
- [x] `--dry-run` on that case prints an explicit adopt notice, e.g.
  `adopt index.md (preserving N existing lines)` — distinct from `update`
- [x] New `--replace` flag for the old overwrite behavior, clearly documented
- [x] Case-insensitive FS: when `INDEX.md` is the same file as `index.md`,
  the generator must target it knowingly — keep the on-disk filename casing
  on adopt, and the adopt/preserve semantics apply (this was one careless
  apply away from destroying mapl-memory's 36 KB curated INDEX.md)

### 2. Malformed-file policy (generator half of RB-3) [3/3]

- [x] `okf index` / `okf log` / `madr toc` skip files with unparseable
  frontmatter with a per-file stderr warning and continue (today: hard abort
  exit 2 on the FIRST bad file anywhere in the vault)
- [x] Subtree scope is honored in the pre-scan: `okf index rdp` must not die
  on a bad file in `iterations/` (ff-rdp B3 repro)
- [x] Exit codes: 0/1 keep their drift semantics; 2 only for real I/O or
  config errors

### 3. Lint-clean generated output [2/2]

- [x] Emit a blank line after `<!-- okf:index:begin -->` (and symmetrically
  before `end` if needed) so generated files pass MD022 — kills the
  `lint --fix` ↔ `okf index` revert ping-pong (3 agents hit it)
- [x] Sweep all generator templates (`okf index`, `okf log`, `madr toc`) so a
  freshly generated file has zero NEW violations under the vault's default
  lint config (long-title MD013 excepted — document that)

### 4. Generator scoping & case-insensitive matching [2/2]

- [x] New `[okf] ignore = ["glob", ...]` honored by `okf index`/`okf log`
  generation (independent of `[lint] ignore`): stop generating into
  `_template/`, `test/fixture-vault/` and friends (df-own-kb U3, mapl UX-5)
- [x] Reserved-file detection and `[schema] exempt` glob matching are
  case-insensitive on case-insensitive filesystems (auto-detected, same
  policy as `[links] case_insensitive`): `INDEX.md`/`LOG.md` are recognized
  as the reserved files they physically are (mapl BUG-2)

### 5. Text output & misc [2/2]

- [x] Fix `--format text` rendering for `okf index` / `okf log` / `madr toc`
  results: proper per-file lines instead of the mis-nested
  `files: action: create` key dump (3 agents)
- [x] `SCHEMA missing required property` violations report
  `autofixable: false` when `--fix` cannot synthesize a value (mapl BUG-3)

### 6. Tests [6/6]

- [x] e2e: adopt round-trip — marker-less file with prose → apply → all
  original lines present + managed region + second apply idempotent +
  dry-run exit codes correct at each step
- [x] e2e: `--replace` overwrites; default never does
- [x] e2e: malformed file in vault → generators skip-warn, scoped run
  unaffected; exit codes asserted
- [x] e2e: generated index/log/toc files lint clean (MD022) in the same run
  as `lint --fix` (no ping-pong: fix → regenerate → dry-run exits 0)
- [x] Case test: uppercase reserved files recognized (runs on macOS/Windows
  CI legs; skipped with a note on case-sensitive Linux)
- [x] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 7. Docs sync (same PR) [1/2]

- [x] `okf index --help` documents adopt/`--replace`/ignore; README OKF
  section updated; okf skill template's maintenance loop reflects adopt
  semantics
- [ ] Retrospective task: adapt iteration-174/175 plans to what landed here
  - [deferred — follow-up: iter-174]

## Acceptance criteria

- [x] Adopt preserves a hand-written index verbatim — `okf_index_adopts_marker_less_index_preserving_body`
- [x] Marker-less adopt logic keeps every existing byte — `splice_adopts_marker_less_body`
- [x] Scoped run survives a malformed out-of-scope file — `okf_index_scoped_ignores_out_of_scope_malformed_file`
- [x] Malformed concept is skipped with a warning, not aborted — `okf_index_skips_malformed_file_with_warning`
- [x] No `lint --fix` ↔ `okf index` ping-pong on generated output — `okf_index_generated_output_is_md022_clean`
- [x] Generated managed block carries MD022 blank lines — `managed_block_has_md022_blank_lines`
- [x] `--replace` overwrites while the default adopts — `okf_index_replace_overwrites_default_adopts`
- [x] Case-insensitive FS targets an existing `INDEX.md` — `okf_index_case_insensitive_targets_existing_upper_index`
- [x] `[okf] ignore` keeps generators out of template trees — `okf_index_honors_okf_ignore_config`
- [x] Generator `--format text` renders readable lines — `okf_index_text_output_is_readable`
- [x] Missing-required with no default reports not-autofixable — `missing_required_no_default_is_not_autofixable`
