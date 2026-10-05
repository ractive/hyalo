---
title: "Iteration 316: search discoverability for agents"
type: iteration
date: 2026-10-05
tags: [iteration, search, find, hints, help, docs]
status: in-progress
branch: iter-316/search-discoverability-for-agents
priority: 1
---

# Search discoverability for agents

## Problem

The ranked-search features from iterations 302–304 (grammar, `--granularity section`,
`--facet`, `terms`) are documented only in a 42 KB `find --help` and in the shipped
skill. `hyalo --help` never says PATTERN is a query language, `find -h` has no room for
the grammar, and the dogfood explorers found every feature through the iteration files,
not through help. An agent that only reads help is unlikely to discover section mode or
`OR`. The owner wants the help and hints to teach the features at the moment they are
needed, without any new subcommand or flag. Brainstormed on 2026-10-05 after
[[iterations/iteration-315-remove-field-terms]]; 316 starts after 315 merges because
both edit `find --help`, the zero-result hints and the skill template. DEC number
reserved: **DEC-367**.

## Tasks

- [x] Baseline experiment: a fresh agent with only `hyalo --help` and subcommand help solves three tasks on the own knowledgebase ("find the paragraph about the proximity bonus", "how many planned iterations mention snapshots, by directory", "which stems does `config*` expand to"); the commands it tried and where it got stuck are recorded in `research/search-discoverability-baseline-2026-10-05.md`
- [ ] Teaching hints, all runnable in the existing `->` format: a ranked query of 3+ bare words with 100+ file hits hints `--granularity section` ("the paragraph, not the file"); a zero-result AND query whose every word exists in the dictionary hints the `OR` form and a `"phrase"~N` form; a `--section` filter that matched headings in many files hints `--granularity section` with the words as PATTERN; the did-you-mean and migration hints from 312/315 stay as they are
- [ ] Runnable help pointers: every envelope that says "see X in `hyalo <cmd> --help`" also carries a `-> hyalo <cmd> --help` entry in `hints` (text and JSON), so the next command is one copy away; the 315 help-drift gate keeps the section names honest
- [ ] A ten-line "find 101" block at the top of `find --help` (grammar in five lines, section mode and facets in two, `terms` in one, flags for structure in one) and the identical block in `skill-hyalo.md` and `rule-knowledgebase.md`; a help-drift check that the three copies are byte-identical
- [ ] The top-level `hyalo --help` PATTERN line names the grammar (implicit AND, `OR`, `"phrase"~N`, `prefix*`, `-term`, groups) — the baseline showed flags are already discoverable there, operators are not; add a cookbook line `hyalo terms config`
- [ ] Re-run the experiment on the new binary with a fresh agent, adding a fourth task that needs the grammar ("files that mention snapshot and either incremental or refresh, but not MDN"); record the comparison in [[research/search-discoverability-baseline-2026-10-05]]; decide from it whether the optional trim applies
- [ ] Optional, expected NOT to apply (the baseline subject never opened `find --help`): only if the re-run shows agents reading the long help and losing their way, move OPERATOR TABLE and COMMON MISTAKES from `find --help` into the skill, keeping `--help` under ~15 KB (DEC-367 records the decision either way)
- [ ] Docs in sync: `find --help`, `hyalo --help`, claims paragraph, templates and their bundled copies, CHANGELOG `[Unreleased]`, DEC-367 (why hints and a header rather than a help subcommand, tutorial or man page)

## Acceptance criteria

- [ ] No new CLI flag or subcommand
- [ ] `hyalo --help | grep -c 'OR'` finds the one-line grammar summary; `hyalo find --help | head -12` is the find-101 block and `diff` against the skill's block is empty
- [ ] `hyalo find 'snapshot index stale'` (100+ hits) prints a runnable `--granularity section` hint; `hyalo find 'snapshot zzqq'`-style zero results with existing words hint `OR`; `hyalo find --section Tasks` with many matches hints section mode; every hinted command exits 0 when run
- [ ] Every "see X in `hyalo <cmd> --help`" envelope carries the matching `-> hyalo <cmd> --help` hint
- [ ] Both experiment transcripts are in the research note with the commands tried and the outcome per task
- [ ] fmt, clippy `-D warnings`, `cargo test --workspace -q`, `cargo deny check`, help-drift, jq-recipes, ts-types, pi-package-sync, pi-runtime, bundled-skills and `hyalo lint --strict` green; CI green on three platforms
