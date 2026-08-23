---
title: "Iteration 220 — pre3 dogfood small batch (config, hints, buckets, gates)"
type: iteration
date: 2026-08-23
status: in-progress
branch: iter-220/dogfood-small-batch
tags: [iteration, ux, cli, config, links, lint]
related:
  - "[[dogfood-results/dogfood-v0210-pre3-fix-waves-207-214]]"
  - "[[iterations/iteration-215-anchor-and-broken-links-followups]]"
  - "[[iterations/iteration-216-results-shape-consistency]]"
---

# Iteration 220 — pre3 dogfood small batch

## Goal

Clear the LOW / LOW-MEDIUM findings from
[[dogfood-results/dogfood-v0210-pre3-fix-waves-207-214]] that don't belong
to iterations 217–219, in one batch (precedent: iteration-204).

## Context

Numbers reference the pre3 report. Items already scoped elsewhere are
excluded: broken-links line numbers + Liquid heading slugs
([[iterations/iteration-215-anchor-and-broken-links-followups]]),
`.results` shape unification
([[iterations/iteration-216-results-shape-consistency]]), links perf
([[iterations/iteration-206-links-perf-profiling]]).

## Tasks

- [x] NEW-9: warn when the effective `site_prefix` (derived or configured)
      stripped 0 of N site-absolute links in a `links` run, naming the
      prefix and suggesting `--site-prefix` / config (MDN: `en-us` derived
      vs `/en-US/docs/…` spelled links → 49,772 false broken and a 110 s
      run, silently). Mark derived prefixes as a guess in `hyalo config`
      output beyond the current bare `(derived)`
- [ ] NEW-12: `find --fields links` inventory is complete — a same-file
      fragment link appears whether or not its anchor resolves; the
      broken/ok verdict is a field, not a presence filter
- [x] NEW-13: stop counting path relocations under `case_mismatches` /
      "Case mismatches" — give `[shortest-path]` relocations their own
      bucket and section in text and JSON
- [ ] NEW-15: `summary` and `find --broken-links` agree on what "broken"
      counts: either `summary` gains a distinct broken-anchors figure
      (`Links: 853 total, 0 broken, 3 broken anchors`) or the two
      commands' definitions are documented and cross-referenced in both
      helps
- [ ] UX-2: an exit-code path for broken anchors in CI — fold anchor
      checking into a lint rule or give `find --broken-links` a
      `--strict`-style nonzero exit when findings exist; `links` text
      gains a one-line "N broken anchors — see `find --broken-links`"
      when anchors are broken but targets are not
- [ ] UX-1: bare `lint` summary appends the config-ignored count
      (`68 files checked (318 ignored by [lint] ignore)`), and a `--glob`
      whose matches are all ignored prints the same exclusion warning the
      named-file form already prints — no silent `0 files checked` green
- [x] NEW-17: fix the self-contradictory `--dir .` note at a config root
      (`./.hyalo.toml does not apply, ./.hyalo.toml is in effect`), print
      `config_path` absolute there like everywhere else, and make the
      malformed-config "every value below is a built-in default" note
      acknowledge the salvaged `dir` when one was salvaged
- [x] NEW-17: document (or normalize) that the effective config for
      `--dir <foreign-tree>` depends on the caller's cwd via ancestor
      discovery; at minimum the "no .hyalo.toml — built-in defaults" note
      must not fire when running from inside the tree would find one
- [ ] NEW-18: `views run` emits the same hints as the equivalent
      `find --view`; `lint-rules show` and `task list` stop being hint
      dead ends
- [ ] NEW-18: `fuzzy_fixes` entries carry `col` alongside `line`
- [ ] NEW-18: `backlinks` JSON reports the authored target spelling (or a
      consistently normalized one) — today slashes are stripped but `.md`
      is not
- [x] UX-3: ancestor-adoption stderr note respects `-q` (or fires once
      per run); drop the stderr `warning:` duplicate of the
      malformed-config diagnostic now that `config` output leads with it
- [ ] UX-4: `hyalo read --format json` without `--frontmatter` hints that
      frontmatter is omitted; `hyalo properties` types nested maps as
      `map` instead of `text`
- [ ] NEW-16: heading task-counts render once — when a hand-written
      `[n/m]` disagrees with the computed count, show the computed one
      (optionally: new lint rule flagging the stale hand-written count)
- [x] CHG-1 (found during iter-217): `hyalo changelog add --category
      Fixed --message "<multi-line>"` mis-places the entry when
      `[Unreleased]` has no existing subsection of that category — it
      lands the entry under an already-released section's matching
      heading instead of creating the subsection under `[Unreleased]`,
      and continuation lines are un-indented. Repro on this repo's own
      CHANGELOG.md (no `### Fixed` under `[Unreleased]`):
      `hyalo changelog add --category Fixed --message "$(printf 'l1\nl2')"
      --dry-run`. Create the missing subsection under `[Unreleased]` and
      indent continuation lines
- [ ] Docs sync in same PR: helps touched above, CHANGELOG

## Acceptance criteria

- [ ] MDN with default derived prefix: the 0-stripped warning names
      `en-us` and suggests a fix; `links` output is no longer a silent
      wrong answer
- [ ] A vault whose only defect is a dead anchor can fail CI through a
      documented command/flag
- [ ] `lint` on the own KB shows the ignored-file count; ignored-only
      `--glob` warns
- [ ] `views run` hint parity with `find --view` (iter-213 AC completed)
- [ ] No command prints a self-contradictory config note
- [ ] `changelog add` creates the `[Unreleased]` subsection when absent
      instead of appending to a released section; continuation lines are
      indented

## Non-goals

- `.results` envelope shape unification (iteration-216)
- Broken-links line numbers, Liquid heading slugs (iteration-215)
- Fuzzy candidate generation cost (iteration-206)
