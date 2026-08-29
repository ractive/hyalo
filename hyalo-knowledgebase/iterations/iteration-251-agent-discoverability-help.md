---
type: iteration
title: "Iteration 251 — agent discoverability: short -h pages and zero-result hints"
date: 2026-08-29
status: planned
tags:
  - iteration
  - agent-cli
  - help
branch: iter-251/agent-discoverability-help
---

# Iteration 251 — agent discoverability: short `-h` pages and zero-result hints

## Goal

Owner observation (2026-08-29): agents don't know what hyalo can do — they
use bare-minimum `find` and pipe through `grep`. Two causes measured
against `target/release/hyalo` 0.21.0: `hyalo -h` is 7.7 KB and
`hyalo find -h` is 12.3 KB (`--help`: 29 KB / 24 KB), because no flag doc
has a first-line/rest split so clap's short help prints whole paragraphs,
and the ~1.9 KB global-options block is repeated on all 27 subcommands.
And when a query returns nothing, text mode prints a bare `No results` and
JSON has `hints: []` — the moment an agent most needs a next step.
Source: the axi.md principles review (2026-08-29, see decision log) —
"concise per-command help" and "definitive empty states". Targets are the
two draft pages (2.4 KB each) attached to that review; they are the
acceptance shape, not a suggestion.

**No new flags.** `-h` already exists; this is content and layout.

## Tasks

- [ ] Every `#[arg]`/`#[command]` doc comment gets a one-line first
      paragraph (clap `help`) followed by a blank line and the existing
      text (clap `long_help`). Target: `hyalo <cmd> -h` ≤ 3 KB for every
      subcommand; `--help` unchanged in content.
- [ ] Global options: one-line short help each; the `--jq` limits paragraph
      lives only in `--help`. On subcommands, `-h` shows globals as a single
      pointer line (`Global: --format --jq --count --no-hints -q — see
      hyalo -h`) — via the existing help template machinery in
      `cli/help.rs`, not a new flag.
- [ ] Top-level `-h`: commands grouped by intent (Read / Write / Config &
      scaffolds), two per line where they pair naturally, one-line
      descriptions naming the capability families (e.g. `find`: "BM25 text,
      regex, property, tag, task, section, title, glob, link-graph
      filters; sort, limit, --fields, --view, --count"). Keep a short
      "Start here" block of ≤ 8 examples that each **compose** 2–3
      features — the current 40 single-flag examples move to `--help`.
- [ ] `find -h`: flags grouped Filters / Output; the property-operator
      line, dot-path note, `--fields` values and `--sort` keys stay (these
      are exactly where agents fall back to grep). ≤ 8 composed examples;
      last line points at `find --help`.
- [ ] Zero-result hints: when `total == 0`, emit 1–3 hints — drop the most
      selective filter (re-run with one filter removed), `hyalo properties`
      / `hyalo tags` to list observed values, and for `--property K=V` a
      did-you-mean over the observed values of K when edit distance is
      small. Text mode: `No results for --property status=x --tag y`
      (echo the effective filters). JSON: same hints in the envelope, with
      `writes: false`.
- [ ] `check-help-drift` / `check-command-reference` / `check-bundled-skills`
      xtask gates updated to the new shape; SKILL.md (bundled + pi-package,
      keep in sync via `just sync-pi-package`) trimmed to workflow +
      pitfalls and told to use `-h` first, `--help` for syntax detail.
- [ ] e2e tests: byte-size ceilings for `-h` on every subcommand (loop over
      `hyalo help` list); zero-result hint presence in text and JSON;
      did-you-mean fires for a one-character typo and not for an unrelated
      value.
- [ ] CHANGELOG `[Unreleased]` → Changed.

## Acceptance criteria

- [ ] `hyalo -h` ≤ 2.5 KB, `hyalo find -h` ≤ 3 KB, every other
      `hyalo <cmd> -h` ≤ 3 KB; every capability named on the current pages
      is still named.
- [ ] `hyalo find --property status=nonexistent` prints a non-empty hint
      block in text and a non-empty `hints` array in JSON.
- [ ] `--help` output loses nothing; all xtask gates and CI green.

## Non-goals

- Any change to JSON result shape or defaults —
  [[iterations/iteration-252-find-result-shape]].
- TOON or any third output format; errors on stdout; content-first bare
  invocation; SessionStart hooks (rejected in the axi review).

## Links

- [[iterations/iteration-246-help-coherence-review-followups]]
- [[iterations/iteration-252-find-result-shape]]
