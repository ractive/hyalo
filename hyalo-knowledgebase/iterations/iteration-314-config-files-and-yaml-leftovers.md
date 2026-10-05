---
title: "Iteration 314: config, files and YAML leftovers"
type: iteration
date: 2026-10-04
tags: [iteration, schema, config, yaml, init, lint, dogfooding]
status: completed
branch: iter-314/config-files-and-yaml-leftovers
priority: 2
related:
  - "[[dogfood-results/dogfood-v0250-pre-search-roadmap-2026-10-04]]"
---

# Config, files and YAML leftovers

## Problem

[[dogfood-results/dogfood-v0250-pre-search-roadmap-2026-10-04]] found that a schema error is reported as `malformed: true` by `hyalo config` but
is not a gate refusal for `lint` and `views run` (UX-2), that a literal backslash in a
filename aborts batch `mv`/`set` with exit 2 (BUG-11), that `init --codex` writes LF
into a CRLF `AGENTS.md` (BUG-12), that `1_000`, `0X1F` and `0b101` still read as
integers under the YAML 1.2 core contract (BUG-14), that `find`/`summary` and `lint`
disagree on whether a non-UTF-8 note is readable (BUG-19), and a handful of help and
claims drifts. DEC numbers reserved for this iteration: **DEC-362 to DEC-365**.

## Tasks

- [x] UX-2: a `[schema]` that fails to load is refused by the gate commands (`lint`, `find --strict`, `views run`) with the DEC-290 envelope exactly like a TOML syntax error, or `hyalo config` stops reporting it under `malformed`; one rule, recorded as a DEC
- [x] BUG-11: a filename containing a literal backslash is reported with its real name on every platform where it is legal, and never produces exit 2; batch `mv`/`set` skip it with a `skipped_detail` reason or refuse with exit 1 naming the file (DEC)
- [x] BUG-12: `init --codex` applies the CRLF detection that `init --claude` applies to `CLAUDE.md` to `AGENTS.md`; e2e test with a CRLF host file
- [x] BUG-14: `1_000`, `0X1F` and `0b101` read as strings (YAML 1.2 core), matching what `set` already quotes; DEC-350 amended; `properties-typed` and `--property` tests
- [x] BUG-19: `find`, `summary` and `lint` agree on a non-UTF-8 note: either all three skip and count it, or lint reads it lossily like find does (DEC)
- [x] HYALO005 names the offending key and says `{{…}}` is not YAML when the unparsable value is a template placeholder
- [x] `lint-rules -h` and `types -h` say `--count` applies to `list` only; `set --help` coercion table states that an unquoted ISO date is written plain and reads back as `date` without a schema
- [x] Claims paragraph: `.gitignore` is honoured only inside a git repository while `.ignore` always is (DEC-342 amended)
- [x] Docs in sync: help texts, `.claude/CLAUDE.md`, `skill-hyalo.md`, `rule-knowledgebase.md`, CHANGELOG `[Unreleased]`, decision-log entries

## Acceptance criteria

- [x] With an `enum` without `values`, `hyalo lint`, `hyalo find --strict` and `hyalo views run <view>` behave identically to the unclosed-`[lint` case
- [x] `notes/back\\slash.md` in a batch `mv --dry-run` exits 0 or 1, never 2
- [x] A CRLF `AGENTS.md` stays pure CRLF through `init --codex`, re-init and `deinit`
- [x] `d: 1_000` reads as `{"type": "text"}`; `hyalo set f.md --property x=1_000` round-trips unchanged
- [ ] fmt, clippy `-D warnings`, `cargo test --workspace -q`, `cargo deny check`, help-drift, `check-jq-recipes` and `hyalo lint --strict` green; CI green on three platforms

## Outcome

Every task and every AC except the CI-platform half of the last one is done and verified locally:

- **UX-2 (DEC-362)**: `run.rs`'s gate-refusal check now consults `schema_invalid` whenever
  `Commands::gates()` is true (lint/`--strict`, `find --strict`, `views run`), never for a
  plain write — preserving DEC-290's carve-out. Before: `lint` exited 0 with a warn-level
  `SCHEMA` row and `views run open` ran to completion on an `enum` with no `values`; only
  `lint --strict`/`find --strict` refused. After: all four commands refuse identically with
  the DEC-290 "unusable .hyalo.toml" envelope naming the diagnostic, matching a TOML syntax
  error. Four existing tests that pinned the old per-violation/exit-0 shape were rewritten
  (`lint.rs`, `schema_enum_requires_values.rs`); new tests cover all four gates plus the
  plain-write/plain-`find` carve-out (`config_trust.rs`, `schema_enum_requires_values.rs`).
- **BUG-11 (DEC-363)**: `native_separator_to_forward_slash` gates the `\`→`/` rewrite behind
  `cfg!(windows)` everywhere a real OS path (not user-typed glob/link/CLI text) is converted
  to a display/lookup string — `discovery.rs`, `find/mod.rs`, `backlinks.rs`,
  `prepared/selection.rs`, `commands/mv.rs`, `link_rewrite.rs`. `notes/back\slash.md` now
  displays, resolves via `--file`/`--glob`, lints cleanly, and moves/sets correctly in a batch
  (`mv --glob`/`set --glob` previously crashed at exit 2; both now fully succeed, not just
  avoid the crash).
- **BUG-12**: `init --codex` detects `AGENTS.md`'s dominant line ending and converts the
  managed block to match before splicing, mirroring `init --claude`. E2e test
  `codex_preserves_crlf_line_endings_in_agents_md` pins install/re-init/deinit.
- **BUG-14 (DEC-364, amends DEC-350)**: a cheap textual pre-check
  (`might_have_yaml11_int_form`) adds a second trigger for the slow span-based re-parse
  (serde-saphyr resolves `1_000`/`0X1F`/`0b101` straight to an integer with no float detour,
  so DEC-350's float-only trigger never fired); `node_to_value`'s `Number` arm now re-checks
  its source text against the core-int grammar the same way the `Float` arm already did.
  Before/after: `find --fields properties-typed` on `d: 1_000` → `{"type": "number", "value":
  1000}` before, `{"type": "text", "value": "1_000"}` after; `find --property p=31` matched
  `p: 0X1F` before, matches nothing after.
  **PR review round:** the first version over-corrected — `resolve_core_int` only matched
  the decimal pattern, so once the slow path triggered on a true YAML 1.1 form elsewhere in
  the same block, a co-occurring core-valid `0x1F`/`0o17` (lowercase prefix) was *also*
  wrongly turned into a string. Fixed by teaching `resolve_core_int` all three of core's
  integer forms (decimal, lowercase `0x…` hex, lowercase `0o…` octal, DEC-326-style sign
  handling down to the `i64::MIN` boundary through any radix), and widening the trigger to
  also catch `0o`/`0O`. `0x1F`/`0o17` now correctly stay numbers (`find --property h=31`
  matches `h: 0x1F`); only `1_000`/`0X1F`/`0b101` (and `0O17`) read as strings. DEC-364 was
  rewritten to name exactly those four forms. `set --property h=0x1F` (CLI-typed) still
  writes a *quoted* string — verified as correct, not a bug: `set`'s own coercion never
  parsed hex input as an integer (unchanged by this fix), so "0x1F" is a string value, and
  quoting it is what keeps it round-tripping as that string now that plain `0x1F` reads back
  as 31. New tests: `resolve_core_int_accepts_the_core_schema_pattern_only` (extended),
  `core_hex_and_octal_int_forms_stay_numbers_alongside_yaml11_strings`,
  `core_hex_and_octal_int_forms_stay_numbers` (e2e).
- **BUG-19 (DEC-365)**: `scan_one_file` returns its own `valid_utf8` instead of only being
  inferred from "no BM25 tokens," so `find`/`summary` report and count the same skip `lint`
  already refused the file over, on every scan shape including the minimal `--fields file`
  listing. The file still appears in listings and still answers `--file` (DEC-301).
- **HYALO005**: `friendly_parse_error` takes the frontmatter source text and, when a parse
  error's location sits on a line holding a `{{...}}` placeholder, names the key and says
  plainly it is not YAML instead of the parser's bare "unexpected end of input."
- **Doc-only fixes**: the `.gitignore`-needs-a-git-repo caveat (verified empirically: a
  `.gitignore` is ignored outside a `.git` tree, `.ignore` is not) is now stated in
  `.claude/CLAUDE.md`, `rule-knowledgebase.md`, `skill-hyalo.md` and the `GITIGNORE:` CLI help
  block (DEC-342 amended); `set --help`'s date-coercion line no longer claims a schema is
  needed to type a date (verified: it already isn't). `lint-rules -h`/`types -h` already said
  `--count` applies to `list` only (checked against the current binary) — no code or doc
  change was needed there; the dogfood item appears already resolved upstream.

Gates, all green locally: `cargo fmt` (clean diff beyond reformatting), `cargo clippy
--workspace --all-targets -- -D warnings` (zero warnings), `cargo test --workspace -q`
(2458 + 1678 + 256 + 71 tests + doctests, 0 failures), `cargo deny check` (advisories/bans/
licenses/sources ok — only pre-existing duplicate-crate warnings), `xtask check-help-drift`,
`check-jq-recipes`, `check-ts-types` (no npm/pi regen needed — no serialized struct shape
changed, only internal logic and help text), and `./target/release/hyalo lint --strict`
on this vault (exit 0; one `MD031` self-inflicted by a DEC entry's fenced code block was
fixed with `lint --fix`). The CI-platform half of the last AC is for the orchestrator to
confirm after the PR's checks run.

Implementation was split with two `fork` subagents dispatched for research that ended up
doing (and finishing) most of the BUG-11/BUG-12/BUG-14/BUG-19 implementation themselves
before hitting their turn limits; this session verified, extended (UX-2's run.rs gate,
HYALO005, the remaining BUG-11 call sites in `mv.rs`/`backlinks.rs`, four pre-existing
tests broken by UX-2's new refusal shape), and closed out docs/DECs/CHANGELOG/gates.
