---
title: Iteration 173 — generator safety (non-destructive adopt, skip-and-warn, case, clean output)
type: iteration
date: 2026-07-17
tags:
  - iteration
  - okf
  - generators
  - fix-wave
status: planned
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

### 1. Non-destructive adoption (RB-2, data loss)

- [ ] `okf index --apply` on an existing `index.md` WITHOUT okf markers:
  preserve the entire existing body and append the managed region (markers +
  generated content) after it; never drop content (mapl repro: H1 + prose +
  manual list must all survive)
- [ ] `--dry-run` on that case prints an explicit adopt notice, e.g.
  `adopt index.md (preserving N existing lines)` — distinct from `update`
- [ ] New `--replace` flag for the old overwrite behavior, clearly documented
- [ ] Case-insensitive FS: when `INDEX.md` is the same file as `index.md`,
  the generator must target it knowingly — keep the on-disk filename casing
  on adopt, and the adopt/preserve semantics apply (this was one careless
  apply away from destroying mapl-memory's 36 KB curated INDEX.md)

### 2. Malformed-file policy (generator half of RB-3)

- [ ] `okf index` / `okf log` / `madr toc` skip files with unparseable
  frontmatter with a per-file stderr warning and continue (today: hard abort
  exit 2 on the FIRST bad file anywhere in the vault)
- [ ] Subtree scope is honored in the pre-scan: `okf index rdp` must not die
  on a bad file in `iterations/` (ff-rdp B3 repro)
- [ ] Exit codes: 0/1 keep their drift semantics; 2 only for real I/O or
  config errors

### 3. Lint-clean generated output

- [ ] Emit a blank line after `<!-- okf:index:begin -->` (and symmetrically
  before `end` if needed) so generated files pass MD022 — kills the
  `lint --fix` ↔ `okf index` revert ping-pong (3 agents hit it)
- [ ] Sweep all generator templates (`okf index`, `okf log`, `madr toc`) so a
  freshly generated file has zero NEW violations under the vault's default
  lint config (long-title MD013 excepted — document that)

### 4. Generator scoping & case-insensitive matching

- [ ] New `[okf] ignore = ["glob", ...]` honored by `okf index`/`okf log`
  generation (independent of `[lint] ignore`): stop generating into
  `_template/`, `test/fixture-vault/` and friends (df-own-kb U3, mapl UX-5)
- [ ] Reserved-file detection and `[schema] exempt` glob matching are
  case-insensitive on case-insensitive filesystems (auto-detected, same
  policy as `[links] case_insensitive`): `INDEX.md`/`LOG.md` are recognized
  as the reserved files they physically are (mapl BUG-2)

### 5. Text output & misc

- [ ] Fix `--format text` rendering for `okf index` / `okf log` / `madr toc`
  results: proper per-file lines instead of the mis-nested
  `files: action: create` key dump (3 agents)
- [ ] `SCHEMA missing required property` violations report
  `autofixable: false` when `--fix` cannot synthesize a value (mapl BUG-3)

### 6. Tests

- [ ] e2e: adopt round-trip — marker-less file with prose → apply → all
  original lines present + managed region + second apply idempotent +
  dry-run exit codes correct at each step
- [ ] e2e: `--replace` overwrites; default never does
- [ ] e2e: malformed file in vault → generators skip-warn, scoped run
  unaffected; exit codes asserted
- [ ] e2e: generated index/log/toc files lint clean (MD022) in the same run
  as `lint --fix` (no ping-pong: fix → regenerate → dry-run exits 0)
- [ ] Case test: uppercase reserved files recognized (runs on macOS/Windows
  CI legs; skipped with a note on case-sensitive Linux)
- [ ] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 7. Docs sync (same PR)

- [ ] `okf index --help` documents adopt/`--replace`/ignore; README OKF
  section updated; okf skill template's maintenance loop reflects adopt
  semantics
- [ ] Retrospective task: adapt iteration-174/175 plans to what landed here

## Acceptance criteria

- [ ] Running the documented conversion flow on a copy of mapl-memory's real
  `INDEX.md` loses zero bytes of hand-written content
- [ ] `okf index rdp` on the ff-rdp branch succeeds despite the malformed
  iteration file elsewhere in the vault
- [ ] `lint --fix && okf index --apply && okf index --dry-run` exits 0 (no
  ping-pong) on a fixture vault
- [ ] Generated files introduce no new lint violations on a default config
