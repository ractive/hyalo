---
title: "Dogfood v0.16.0 — iter-157/158/159 + PR #186 verification, links-fix frontmatter no-op"
type: research
date: 2026-07-10
status: active
tags:
  - dogfooding
  - iter-157
  - iter-158
  - iter-159
  - links
  - index
related:
  - "[[dogfood-results/dogfood-v0160-firefox]]"
  - "[[iterations/iteration-157-lazy-stem-map]]"
  - "[[iterations/iteration-158-review-fixes]]"
  - "[[iterations/iteration-159-pi-integration]]"
  - "[[reviews/codebase-review-2026-07-10]]"
  - "[[research/feature-gaps-2026-07-10]]"
---

# Dogfood v0.16.0 — iter-157/158/159 + PR #186

Binary: `hyalo 0.16.0 (3ca89718409c 2026-07-08)`. KBs exercised: own KB
(307 files), MDN (`../mdn/files/en-us`, 14,375 files), GitHub Docs
(`../docs/content`, 3,710 files). VS Code docs not present on this machine.
Scratch vaults for mutation/edge tests.

## New feature verification

### iter-157 — lazy + index-seeded stem map — WORKING

Indexed queries on MDN no longer pay a disk walk. 114 MB index, built in
2.8 s: property query 0.42 s, BM25 0.41 s, `summary` 0.64 s. Non-indexed
parallel scan fallback: 1.28 s. F-1 from the firefox report stays closed at
8× the corpus size.

### iter-158 — review fixes — ALL SPOT-CHECKS PASS

- **C-1 BOM**: `set` on a BOM-prefixed file preserves the BOM and edits
  cleanly (hexdump-verified).
- **H-3 symlink escape**: `mv target.md --to escape/stolen.md` through an
  out-of-vault symlink → `Error: target path resolves outside vault boundary`.
- **H-4 frontmatter rewrite on mv**: `related: ["[[target]]"]` and body link
  both rewritten on `mv`. ✓
- **H-6 JSON errors**: `read nonexistent.md --format json` →
  `{"error": "file not found", "path": ...}`. ✓
- **H-9 fenced checkbox**: `task toggle --line` on a checkbox inside a code
  fence → `Error: line 7 is not a task`. ✓
- **Atomicity**: 20 concurrent `set` on one file → consistent last-writer-wins
  file, no corruption.

### iter-159 — `init --pi` / `deinit` — WORKING

`init --pi --dir kb` creates `.hyalo.toml`, the two `.pi/skills/` SKILL.md
files, `.pi/extensions/hyalo.ts`, `.pi/package.json`. `deinit` removes it and
skips absent claude artifacts with clear `skipped`/`removed` lines.

### PR #186 — `--first-only` existing links + SIGPIPE — WORKING (but see BUG-2)

- A file containing `[[fake-login]]` no longer gains a second link for a later
  plain-text `fake-login` mention under `--first-only`; first-of-two plain
  mentions in a link-free file is still proposed. Exactly as specified.
- `find --format json | head -c 200` and `lint --format text | head -2` exit
  quietly — no panic, empty stderr, on both JSON and text paths.

## Bug regression testing (firefox report)

- **F-3 (duplicate error on datetime mismatch) — STILL OPEN, now TRIPLE.**
  A `type = "datetime"` property with value `not-a-date` produces one SCHEMA
  error **plus** HYALO003 (date heuristic — wrong rule for a datetime-typed
  property) **plus** HYALO004. Three reports, one defect.
- **F-4 (`tasks` alias) — STILL OPEN** (LOW). clap tip suggests `task`.
- **F-5 (`links --file`) — STILL OPEN** (LOW).
- **F-6 (lint wall of warnings) — IMPROVED VIA HINTS.** On GitHub Docs, lint
  emits a hint naming the noisiest rule
  (`lint-rules show MD010 … # Tune MD010 if too noisy on this KB`). No
  `--by-rule` summary table yet.

## Bugs found

### BUG-1: `links fix --apply` silently no-ops on frontmatter wikilinks while reporting them applied (HIGH)

Repro (minimal): `a.md` with `related: ["[[wrong/real-target]]"]` **and** a
body link `[[wrong/real-target]]`; real file at `sub/real-target.md`.

```text
$ hyalo links fix --apply
Broken links: 2 / Fixable: 2 / Applied: yes
  a.md line 1: "wrong/real-target" → "sub/real-target.md"
```

Body link rewritten; **frontmatter link untouched**; next dry run reports the
same fix as fixable again. On the own KB, 13 of 15 "applied" fixes were
frontmatter (`related:`) links — all no-ops. Impact: an agent driving a
`links → links fix --apply → links` loop never converges and believes it
succeeded; "Applied: yes" is a false report. Also: the detail list
deduplicates by (source, old, new), so the frontmatter and body occurrence
show as one line — you cannot see what was actually written.

Contrast: `mv` rewrites frontmatter wikilinks correctly (H-4 fix). `links fix`
needs the same write path. Root cause: `build_replacements_for_file`
(`crates/hyalo-core/src/link_fix.rs:919`) walks the body only — it skips
frontmatter by design — so frontmatter FixPlans yield no Replacement, while
the CLI reports the original FixPlan list (not the actual RewritePlans) as
applied.

### BUG-2: Drill-down hints drop invocation flags (MEDIUM)

Two instances, one class:

- `links auto --first-only` (dry run) → hint says
  `hyalo links auto --apply` **without `--first-only`**. Pasting the hint
  applied 3 links where the dry run promised 1 — reintroducing exactly the
  double-linking PR #186 fixed. `--exclude-target-glob` is dropped too.
  Root cause: `hints_for_links_auto` (`crates/hyalo-cli/src/hints.rs:1520`)
  deliberately rebuilds the command "preserving all scope-narrowing flags"
  and handles `--min-length`/`--file`/`--exclude-title`/`--glob`, but
  `first_only` and `exclude_target_glob` were never added — PR #186 touched
  `args.rs` without extending the hint builder. (`--min-length 3` elision is
  fine — 3 is the default.)
- `create-index --index-file <outside-path> --allow-outside-vault` → the
  follow-up hint suggests `hyalo drop-index --dir <vault>` **without
  `--index-file`**, which deletes the *in-vault* `.hyalo-index` (a different
  index) instead of the one just created. This actually deleted a
  pre-existing MDN in-vault index during this session (rebuilt afterwards).

Hints are the agent-facing navigation surface; they must preserve
behavior-changing flags of the invocation they derive from.

### BUG-3: Fuzzy link-fix default threshold accepts wrong matches on structured names (MEDIUM)

On the own KB, `links fix` proposed
rewriting `iterations/done/iteration-132-mv-wikilinks` to
`iterations/done/iteration-02-links.md` at confidence 0.896 (no file named
`iteration-132-mv-wikilinks` ever existed;
`iteration-02-links` is unrelated). Long shared prefixes
(`iterations/done/iteration-`) inflate Jaro-Winkler; two different iterations
clear the 0.8 default easily. `ShortestPath` fixes were 15/15 correct,
`FuzzyMatch` 0/2 — strategies separate cleanly. Suggestion: score fuzzy
similarity on the basename/slug (or down-weight the common prefix), and/or
add `--strategy` so `--apply` can be restricted to exact-basename moves.
Workaround that worked: `--threshold 0.95`.

### BUG-4: Stale snapshot index diverges silently (MEDIUM)

`set` without `--index` does **not** patch an existing in-vault
`.hyalo-index`; a later `find --index` returns pre-edit values, and
`summary --index` misses files created outside hyalo. No staleness signal of
any kind. Either auto-patch the in-vault index on every mutation (it is
already found and loaded when `--index` is passed), or warn when index build
time < newest file mtime.

### BUG-5: `lint` MD-rule line numbers are body-relative, off by the frontmatter length (MEDIUM)

Every MD* violation reports `line N` where N is counted from the start of the
**body**, not the file. Verified on two corpora: this report (frontmatter
12 lines, MD040 reported line 67, real fence at line 79) and GitHub Docs
(frontmatter 14 lines, MD010 reported 197, real tab at 211). Meanwhile
SCHEMA/HYALO00x findings report `line 1` regardless of where the property
sits — two different conventions in one output. `lint --fix` itself edits the
**correct** lines (verified with MD009/MD012), so this is purely a reporting
defect — but any agent or editor jumping to the reported line lands
frontmatter-length lines too early.

### BUG-6: Multi-line property values print raw in text output (LOW)

`set --property $'evil=x\nmalicious: injected'` is stored safely
(`"x\nmalicious: injected"`, YAML-quoted — write path is injection-safe), but
`find --fields properties --format text` prints the value with a literal
newline, so `malicious: injected` renders as a fake sibling property. An LLM
reading text output would misparse this. Escape or indent continuation lines.

## UX issues

### UX-1: `mv` destination is `--to`, not positional (LOW)

`hyalo mv a.md b.md` fails; coreutils muscle memory (and LLM priors) expect
positional. The clap error does show usage. A COMMON MISTAKES entry in
`mv --help` (like `find`'s) — or accepting a positional destination — would
remove the stumble.

### UX-2: `new` has no way to set title/properties at creation (LOW)

`hyalo new --type note --file x.md` scaffolds `title: TBD`; a natural
`--title`/`--property` override would save one `set` round-trip in the
agent fill-in loop the help text itself describes.

## What worked well

- **Help texts are genuinely LLM-friendly.** 19/21 subcommands have
  EXAMPLES, most have OUTPUT + SIDE EFFECTS contracts; `find --help` has a
  COMMON MISTAKES section (`~=` vs `=~`, `--title` vs `title~=`);
  `new --help` explicitly documents the agent fill-in loop ("designed to fail
  `hyalo lint`"). Error messages enumerate valid values
  (`unknown field "file": valid fields are all, properties, …`) and errors are
  structured JSON under `--format json`, including jq syntax errors with a
  `cause` field.
- **Hints quote paths correctly** — a path with spaces, parens and umlauts
  came back copy-pasteable in the drill-down hint.
- **Robustness edges all held**: CRLF files round-trip byte-exact through
  `set`/`task toggle`; unicode+spaces+parens paths fine; 9.3 MB body file
  reads/searches fine; YAML injection via `set` is neutralized by quoting;
  malformed `.hyalo.toml` produces a caret-diagnostic warning; empty vault
  output is calm and complete.
- **`properties` type inference** flags data quality
  (`priority: mixed (87 text, 6 number)`) — found a real inconsistency in the
  own KB.
- **jq dashboards**: `--jq '.results | map(.properties.status) | group_by(.) |
  map({(.[0]): length}) | add'` → `{"completed":121,…}` in one call.
- **Real repair value**: 15 genuinely broken own-KB links (done/↔root moves)
  fixed via `links fix --apply --threshold 0.95` — 2 body-link fixes landed;
  the 13 frontmatter ones exposed BUG-1.

## Performance

| KB | Files | Command | Time |
|---|---:|---|---:|
| MDN | 14,375 | create-index (114 MB) | 2.8 s |
| MDN | 14,375 | find --property (indexed) | 0.42 s |
| MDN | 14,375 | find BM25 (indexed) | 0.41 s |
| MDN | 14,375 | summary (indexed) | 0.64 s |
| MDN | 14,375 | find --property (scan) | 1.28 s |
| GitHub Docs | 3,710 | summary (scan) | 0.47 s |
| GitHub Docs | 3,710 | lint (full corpus) | 0.87 s |
| own KB | 307 | everything | < 0.1 s |

No regressions vs. prior baselines; indexed short-query latency is an order
of magnitude better than the pre-157 firefox numbers.

## Feature gaps noticed while working

- **Nested-map property queries**: GitHub Docs `versions: {fpt: '*', ghes: …}`
  is not reachable via `--property versions.fpt=*`; only existence and regex
  over the serialized map work.
- **`lint --by-rule` summary** (F-6 follow-up): one line per rule with counts,
  for triage on stock corpora.
- **`links fix --strategy` filter** (see BUG-3).
- **Index staleness signal** (see BUG-4).

## Verdict

All four recent changes (iter-157/158/159, PR #186) hold up under adversarial
testing, and the iter-158 hardening survives every probe thrown at it. The
session's headline is BUG-1: `links fix --apply` falsely reports frontmatter
fixes as applied — the last remaining frontmatter-vs-body asymmetry in the
link machinery, and poison for agent fix-loops. BUG-2 (flag-dropping hints)
is the most agent-hostile behavior found: the tool's best feature (hints)
actively rewrites the user's intent.
