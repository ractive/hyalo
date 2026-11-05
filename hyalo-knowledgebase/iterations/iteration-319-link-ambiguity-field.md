---
title: "Iteration 319: link ambiguity as a first-class field"
type: iteration
date: 2026-11-05
tags:
  - links
  - find
  - mv
  - lint
status: completed
branch: iter-319/link-ambiguity-field
priority: 2
related:
  - "[[backlog/link-ambiguity-as-a-first-class-link-field]]"
  - "[[iterations/iteration-275-alias-semantics-and-mv-guards]]"
  - "[[iterations/iteration-287-typed-typescript-api]]"
  - "[[iterations/iteration-318-regression-dogfood-fixes]]"
---

# Link ambiguity as a first-class field

## Problem

A `[[stem]]` that matches two files and a `[[stem]]` that matches none look identical in
`find --fields links`: both report `path: null` and nothing else. The two have opposite
remedies — a broken link needs a target created or a typo fixed, an ambiguous one needs the
*author* to disambiguate (`[[sub/stem]]`) — and the only command that told them apart was
`links fix`, whose job is to propose writes.

Four places decided "this bare name is ambiguous" four ways: `links fix` (`ambiguous_links`),
HYALO006 ("matches N candidates"), `mv` (`skipped_ambiguous`, three separate guards) and the
catalog resolver behind `backlinks`. Shipping only the reporting half would have added a
fifth. This iteration removes the duplication and reports the result.

DEC number: **DEC-374** (DEC-372 is the 0.25.1 Windows C-runtime decision, DEC-373 the Unicode table exception of PR #396).

## Tasks

- [x] One definition: `CaseInsensitiveIndex::ambiguity` / `stem_ambiguity` / `ambiguous_candidates` / `ambiguous_stem_candidates` in `hyalo-core/src/case_index.rs`, with the shared `bare_stem` target normaliser and `sorted_candidates`; unit tests for stems, aliases, both `[links] aliases` modes, and "an ambiguous stem is never tie-broken by an alias"
- [x] `find --fields links`: `ambiguous: true` and sorted `candidates` on an unresolved bare wikilink that names several files or a doubly claimed alias; neither key on any other link; computed only for links that failed to resolve, so the common link allocates nothing
- [x] Every consumer reads the one definition: `links fix` (`classify_short_form_wikilink`, `ambiguous_links`), HYALO006 (`check_broken_links`), the catalog resolver behind `backlinks` and `mv`'s inbound/outbound/frontmatter/identity guards (`LinkResolver::resolve_stem`), and `discovery::resolve_target`; the four private re-implementations are deleted
- [x] Text mode prints `(ambiguous: a/note.md, b/note.md)` in place of `(unresolved)`
- [x] `ts-rs` derives on `LinkInfo` keep the omission semantics; TypeScript declarations regenerated; same-binary API contract fixtures extended; Pi companions rebuilt and synced; both freshness gates green
- [x] e2e test pinning `find`, HYALO006, `mv` `skipped_ambiguous` and `links fix` against one vault; `--index` and disk scans report identical `candidates`; an ambiguous link still counts as broken in `summary.links.broken`, `find --broken-links` and HYALO006 (DEC-351)
- [x] No snapshot format bump: candidates are computed at resolve time from the case index both scans already build
- [x] Timing of `find --fields links --limit 0` on MDN and GitHub Docs, origin/main against this branch
- [x] PR #399 review round: one normalisation (`?` literal-first, `./` segments) shared by `find`, HYALO006, `links fix`, `mv` and `backlinks`; table-driven parity test over `[[note]]`, `[[note?]]`, `[[What?]]`, `[[my%20note]]`, `[[./note]]`, `[[note#H]]`, `[[note|alias]]` and a frontmatter link; Pi bundles built without the worktree `node_modules` symlink
- [x] Docs in sync: `find --help`, `skill-hyalo.md`, `rule-knowledgebase.md`, the link-record paragraph in `.claude/CLAUDE.md`, CHANGELOG `[Unreleased]`, DEC-374, backlog item completed

## Acceptance criteria

- [x] `find --fields links` reports `ambiguous: true` and sorted `candidates` on an ambiguous target, and neither key on any other link
- [x] `mv`'s `skipped_ambiguous` entries and HYALO006's "matches N candidates" message are derived from the same computation as the field, pinned by one test against one vault
- [x] `--index` and disk scans report identical `candidates` (sorted, vault-relative)
- [x] Text mode prints `(ambiguous: a/note.md, b/note.md)` in place of `(unresolved)`
- [x] An alias two notes claim is ambiguous, `[links] aliases = false/true` does not change whether an ambiguous stem is reported, and `mv --allow-ambiguous`, `self: true` and `property` on `skipped_ambiguous` keep working
- [x] `summary.links.broken`, `find --broken-links` and HYALO006 keep agreeing; an ambiguous link is still broken (`path` stays `null`)
- [x] No new CLI flag or subcommand; fmt, clippy `-D warnings`, `cargo test --workspace -q`, all xtask `check-*` gates and `hyalo lint` green

## Results

`find --fields links --limit 0`, user CPU seconds over five runs, origin/main
against this branch, on a loaded machine (user time rather than wall time):

| Corpus | origin/main | this branch |
|---|---|---|
| MDN (14,375 files) | 1.80–1.86 | 1.81–1.83 |
| GitHub Docs (3,710 files) | 0.55–0.58 | 0.55–0.57 |
| Obsidian Hub (6,519 files) | 0.63–0.65 | 0.61–0.63 |

`summary.links` and the `find --broken-links` file count are identical on all
three. The Hub reports 100 ambiguous links, none on MDN or GitHub Docs, and its
`broken` count stays 162.
