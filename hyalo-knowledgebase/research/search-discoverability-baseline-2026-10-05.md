---
title: "Search discoverability baseline (help-only agent, 2026-10-05)"
type: research
date: 2026-10-05
status: active
tags: [research, search, help, hints, agents]
related:
  - "[[iterations/iteration-316-search-discoverability-for-agents]]"
  - "[[iterations/iteration-315-remove-field-terms]]"
---

# Search discoverability baseline (help-only agent, 2026-10-05)

Experiment for [[iterations/iteration-316-search-discoverability-for-agents]], run before
any help change, against the `main` binary (4e6aff14, field terms still present). The
subject was a Sonnet-class agent allowed to read only `hyalo --help` and subcommand
help, with a budget of 25 read-only commands and three explicit tasks.

## Result: 5 commands, no dead ends

| Task | Commands | Discovery path | Answer |
| --- | ---: | --- | --- |
| The paragraph explaining the proximity bonus | 3 | `hyalo --help`; `find "proximity bonus"` ranked `decision-log.md` first; `--granularity section` (seen in the find synopsis of the top-level help) gave the line range | `decision-log.md`, DEC-338, lines 6667–6687 |
| Planned iterations mentioning "snapshot", by directory | 1 | `--facet dir` is listed in the top-level find synopsis; `find snapshot --property status=planned --property type=iteration --facet dir` | 2, both under `iterations/` |
| Stems that `config*` expands to | 1 | the `terms` one-liner plus its `[PREFIX]` argument; `terms config --limit 5` | config, configur, configexclud, configpath, configfil |

The subject's own note: the top-level help (COOKBOOK + COMMAND REFERENCE) was enough;
no subcommand `--help` was opened. The one inference it had to make was that
`terms PREFIX` is a literal prefix over stems rather than a `config*` wildcard; a
cookbook line `hyalo terms config` would remove it.

## What this does and does not show

- Flag discovery works: `--granularity section`, `--facet` and `terms` are all visible
  in the 35 KB top-level help's find synopsis and command list.
- The grammar was not exercised. None of the three tasks needed `OR`, a phrase with
  slop, a prefix or negation, and the top-level PATTERN line ("returns score and up to
  3 body matches … ranked by distinct query") does not name any operator. The re-run
  must include a task that needs `OR` and negation.
- One subject, one model tier, explicit task wording. A vaguer task ("what does the
  knowledgebase say about proximity?") or a weaker model may behave differently.

## Consequences for iteration 316

- Keep: teaching hints at the moment of a mistake, runnable `-> hyalo find --help`
  pointers, the shared find-101 block.
- Narrow: the `hyalo --help` change is one grammar sentence on the existing PATTERN
  line, not a new paragraph.
- Drop unless the re-run shows otherwise: trimming `find --help`; the subject never
  opened it.
- Re-run with a fourth task that needs the grammar: "files that mention snapshot and
  either incremental or refresh, but not MDN".
