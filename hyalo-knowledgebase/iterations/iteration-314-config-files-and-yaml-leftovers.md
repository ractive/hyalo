---
title: "Iteration 314: config, files and YAML leftovers"
type: iteration
date: 2026-10-04
tags: [iteration, schema, config, yaml, init, lint, dogfooding]
status: planned
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

- [ ] UX-2: a `[schema]` that fails to load is refused by the gate commands (`lint`, `find --strict`, `views run`) with the DEC-290 envelope exactly like a TOML syntax error, or `hyalo config` stops reporting it under `malformed`; one rule, recorded as a DEC
- [ ] BUG-11: a filename containing a literal backslash is reported with its real name on every platform where it is legal, and never produces exit 2; batch `mv`/`set` skip it with a `skipped_detail` reason or refuse with exit 1 naming the file (DEC)
- [ ] BUG-12: `init --codex` applies the CRLF detection that `init --claude` applies to `CLAUDE.md` to `AGENTS.md`; e2e test with a CRLF host file
- [ ] BUG-14: `1_000`, `0X1F` and `0b101` read as strings (YAML 1.2 core), matching what `set` already quotes; DEC-350 amended; `properties-typed` and `--property` tests
- [ ] BUG-19: `find`, `summary` and `lint` agree on a non-UTF-8 note: either all three skip and count it, or lint reads it lossily like find does (DEC)
- [ ] HYALO005 names the offending key and says `{{…}}` is not YAML when the unparsable value is a template placeholder
- [ ] `lint-rules -h` and `types -h` say `--count` applies to `list` only; `set --help` coercion table states that an unquoted ISO date is written plain and reads back as `date` without a schema
- [ ] Claims paragraph: `.gitignore` is honoured only inside a git repository while `.ignore` always is (DEC-342 amended)
- [ ] Docs in sync: help texts, `.claude/CLAUDE.md`, `skill-hyalo.md`, `rule-knowledgebase.md`, CHANGELOG `[Unreleased]`, decision-log entries

## Acceptance criteria

- [ ] With an `enum` without `values`, `hyalo lint`, `hyalo find --strict` and `hyalo views run <view>` behave identically to the unclosed-`[lint` case
- [ ] `notes/back\\slash.md` in a batch `mv --dry-run` exits 0 or 1, never 2
- [ ] A CRLF `AGENTS.md` stays pure CRLF through `init --codex`, re-init and `deinit`
- [ ] `d: 1_000` reads as `{"type": "text"}`; `hyalo set f.md --property x=1_000` round-trips unchanged
- [ ] fmt, clippy `-D warnings`, `cargo test --workspace -q`, `cargo deny check`, help-drift, `check-jq-recipes` and `hyalo lint --strict` green; CI green on three platforms
