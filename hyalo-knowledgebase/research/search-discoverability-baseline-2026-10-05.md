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

## Re-run (new binary)

Run after iteration 316's build was green: one fresh Sonnet-class subject, the same
brief and limits (help and read-only commands only, 25-command budget), and a fourth
task that needs the grammar.

| Task | Commands | Discovery path | Answer |
| --- | ---: | --- | --- |
| The paragraph explaining the proximity bonus | 3 | `hyalo --help`; `hyalo find --help \| grep proximity` (the RANKING blurb); `find "proximity bonus" --granularity section --format json`, crediting the FIND 101 line "one hit per section: the paragraph, not the file" | `decision-log.md`, "DEC-338: Phrase slop and a proximity bonus", lines 6671–6691 |
| Planned iterations mentioning "snapshot", by directory | 2 | `find snapshot --property status=planned --property type=iteration --facet dir` gave 0, and the zero-result hint showed that `status=planned` (6 files) and `type=iteration` (309) do not overlap; with the type filter dropped, `--facet dir` gave `backlog: 3` | No planned iterations exist now; the 3 planned files that mention snapshot are in `backlog/` |
| Stems that `config*` expands to | 2 | `terms --help`; `terms config --limit 5` | config 185, configur 122, configexclud 9, configpath 9, configfil 7 |
| Mentions snapshot and (incremental or refresh) but not MDN | 1 (+1 to verify) | Grammar from the FIND 101 block: `find "snapshot (incremental OR refresh) -mdn" --count` | 24 |

12 commands in total. All four tasks were solved, including the one that needs
the grammar.

### Comparison with the baseline

| | Baseline (old binary) | Re-run (iteration 316) |
| --- | --- | --- |
| Tasks / solved | 3 / 3 | 4 / 4 (adds the grammar task) |
| Commands | 5 | 12 (8 for the three shared tasks: one extra `--help`, one zero-result filter detour) |
| Where the features were found | top-level help only; `find --help` never opened | top-level help, then the FIND 101 block at the top of `find --help`; zero-result hints for the filter detour |
| Operators used | none (no task needed them) | implicit AND, `OR` inside `( )`, `-term`, all on the first try |
| Dead ends | none | `--property status=planned --property type=iteration` (no overlap); `--fields` rejected in section mode |

The subject's own assessment: the FIND 101 block was the single most useful text,
covering `OR` precedence, `( )`/`-`, `--granularity section`, `--facet`, and
`terms config` for prefix stems. Reading it first would have avoided both dead
ends. The zero-result hint that named the counts of the conflicting filters was
faster than trial and error.

### Decision on the optional trim

Not applied (DEC-367). The subject opened `find --help` only to read its head
and to `grep` one keyword, and it got what it needed both times. It never got
lost in the 42 KB page. The trim stays conditional on a transcript showing an
agent reading past FIND 101 and choosing a wrong operator or flag because of
what it read further down.
