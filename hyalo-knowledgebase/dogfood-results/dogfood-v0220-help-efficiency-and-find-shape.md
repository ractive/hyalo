---
title: "Dogfood v0.22.0 — short-help efficiency, -h/--help coherence, find result shape"
type: research
date: 2026-08-30
status: active
tags: [dogfooding, help-text, find-shape, agent-discoverability]
related:
  - "[[dogfood-results/dogfood-v0200-post-247-sweep]]"
  - "[[iterations/iteration-250-mdlint-0161-workaround-strip]]"
  - "[[iterations/iteration-251-agent-discoverability-help]]"
  - "[[iterations/iteration-252-find-result-shape]]"
  - "[[iterations/iteration-253-read-lines-single-pass]]"
---

# Dogfood v0.22.0 — short-help efficiency, -h/--help coherence, find result shape

Binary: `hyalo 0.22.0 (1c36e2bf8a21 2026-08-30)`, built from `main` after
PRs #291–#293 (iterations 250–252). Baseline for regressions:
[[dogfood-results/dogfood-v0200-post-247-sweep]] (2026-08-28).

Focus requested by the owner: (1) byte/token efficiency of the new short help
`-h`, (2) coherence between `-h`, `--help` and behaviour, (3) how the compact
`find` default shape changes agent workflows. Three parallel read-only agents
covered one focus each; mutating commands ran on scratch copies of the KB.

KBs: own KB (427 files, disk), GitHub Docs (`../docs/content`, 3.7K files,
nested YAML, disk), MDN (`../mdn/files/en-us`, 14,375 files, snapshot index
built in scratch with `--allow-outside-vault`, 4.5 s, 121 MB). `../vscode`
not on disk — skipped.

Headline: the 251/252 targets hold — all 52 `-h` pages are under 3 KB
(≈47 KB total vs ≈470 KB for `--help`), and the default `find` payload is
5–20× smaller than `--fields all` on every KB. But the doc-comment split was
done per *physical line*, not per sentence, so **16 one-liners are sentence
fragments on both pages** (COH-6, HIGH), the `find` headline and the root
`--help` JSON cookbook still describe the pre-252 shape (COH-3/COH-4, HIGH),
and one bundled `hyalo-tidy` recipe now **silently returns `[]`** because
`.tasks` left the default shape (FIND-2, HIGH). Prior BUG-1 and UX-1/UX-2 are
fixed; BUG-2, UX-3, UX-5 remain open.

## New Feature Verification

### mdbook-lint 0.16.1, `md047_fix` removed (iter-250) — WORKING

Scratch KB with `crlf-no-eol.md` (no EOL) and `crlf-extra-eol.md` (3 CRLF
EOLs): `lint --fix --rule MD047` run 1 → `fixed 2 · remaining 0 · conflicts 0`;
re-lint clean; run 2 `fixed 0`; tail bytes `0d0a` preserved. Converges in one
run, CRLF not converted.

### Short `-h` pages + zero-result hints (iter-251) — WORKING, with fragments

- Root `-h` 2510 B / 43 lines (≈630 tokens; includes the 130 B vault banner).
  `find -h` 2860 B; every other subcommand ≤ 2382 B; 52 pages sum to 47.4 KB.
  The `Global: …` pointer line is byte-identical on all 52 pages.
- Flag sets are complete on both `-h` and `--help` for all 52 commands; the
  only difference is the intended 8 globals collapsed into the pointer.
- 14 realistic agent tasks attempted from `-h` alone: 12 pass; 2 fail
  (`views run --filenames-only` ignored — HELP-3; `summary --jq
  '.results.file_count'` → null, key is `.files` — HELP-14). Had to open
  `--help` for `--dir` meaning, `task --line` numbering, `set --validate`,
  `mv --property` operators.
- Zero-result output: `No results for --property status=plannd --tag
  iteration` + did-you-mean `status=planned` + drop-filter hint — exactly as
  planned. Typo tips on subcommands and flags everywhere except `hyalo help
  <typo>` (HELP-13).

### Compact `find` default shape, `size`/`lines` (iter-252) — WORKING

Payload bytes, `--format json --no-hints`:

| KB | query | default | `--fields all` | `--fields title` | ratio |
|---|---|---:|---:|---:|---:|
| own | `find --tag iteration --limit 20` | 11,020 | 225,625 | 4,645 | 20.5× |
| own | `find "broken links"` (80+ hits, no limit) | 35,471 | 455,775 | 14,157 | 12.9× |
| own | `find --limit 50` | 24,706 | 121,753 | 11,082 | 4.9× |
| ghdocs | `find --limit 20` | 22,348 | 117,257 | 4,511 | 5.2× |
| ghdocs | `find --limit 50` | 71,675 | 267,663 | 12,288 | 3.7× |
| mdn (idx) | `find --limit 20` | 8,004 | 94,592 | 4,548 | 11.8× |
| mdn (idx) | `find "broken links" --limit 20` | 10,001 | 159,942 | 5,158 | 16.0× |
| mdn (idx) | `find --limit 50` | 19,764 | 188,996 | 10,924 | 9.6× |

The 73 KB → 11.9 KB claim reproduces (11.0 KB with `--no-hints`). GitHub Docs
is the outlier: nested-YAML `properties` are ~1.1 KB/item, so `properties`
dominates and only `--fields title` gets a 20-file listing to own-KB size. An
unlimited pattern search still emits the whole hit set (35 KB on the own KB) —
the win holds only with `--limit`.

- Auto-include is complete for every implying filter (`--section`→sections,
  `--task`→tasks, `--broken-links`→links, `--orphan`/`--dead-end`→links +
  backlinks, `--sort links_count|backlinks_count`→that field), also when
  combined with an explicit `--fields`; text and JSON agree on the field set
  in all 24 combinations tried. The text `fields:` summary line is the best
  discoverability aid in the release.
- `title` is really out of `properties` (0 of 427 files carry
  `.properties.title`); `--fields properties` alone keeps it;
  `properties_typed` keeps it. `--sort title`, `--property 'title~=…'`,
  `set --property title=X` all still work.
- `size`/`lines`: byte-exact between disk, index and `wc -c` on 20 MDN files
  and all edge cases (CRLF, BOM, emoji, no trailing newline, empty,
  zero-byte). `lines` counts an unterminated last line (differs from `wc -l`
  by design; `read --lines` uses the same numbering).
- Views with a pinned `fields` behave like an explicit `--fields`; CLI
  `--fields all` overrides the pin.
- The 2..5-item set-level hint `find --fields all <same filters>  # Include
  the omitted fields (sections, links, tasks, backlinks)` is phrased right.

## Bug Regression Testing

| ID | Status | Evidence |
|---|---|---|
| BUG-1 `task toggle --index` BM25 drift | **FIXED** | scratch: `create-index; task toggle … --all --index`; `find "stale index" --index` vs disk `jq -S .results` → identical, scores equal |
| BUG-2 no-op `set --index` does not refresh a disk-changed entry | **STILL OPEN** (not in 249 scope) | append text to a note; `set … --property status=completed --index` (0 modified); `find zzqqx --index --count` → 1, disk → 2 |
| UX-1 stale-index probe blind on nested vaults | **FIXED** (see FIND-9) | depth-2 file added ≥2 s after `create-index` → `warning: index older than vault…`; depth-5 still silent (documented) |
| UX-2 `links fix --apply-fuzzy` mislabels applied fixes | **FIXED** | `Low-confidence matches (applied at or above confidence 0.8):` … `1 of 2 below the confidence floor 0.8` |
| UX-3 UTF-8 placeholder says "lossy in search", search skips file | **STILL OPEN** | `read bad-utf8.md` placeholder unchanged; `find "line ok"` → `warning: skipping … stream did not contain valid UTF-8` |
| UX-4 explicit file vs `[lint] ignore` | **IMPROVED** | now warns `1 named file excluded by [lint] ignore (not linted)`; still no override |
| UX-5 `new --property` | **STILL OPEN** | `error: unexpected argument '--property'`; `new --help` doesn't point to `set` |
| UX-6 `find '--index …'` needs `-- ` | STILL OPEN (clap) | tip present; see UX-7 |

## Bugs Found

### FIND-2 / COH-14: bundled `hyalo-tidy` recipe silently returns `[]` (HIGH)

`crates/hyalo-cli/templates/skill-hyalo-tidy.md:180`, byte-identical in
`crates/hyalo-cli/templates/pi/skills/hyalo-tidy/SKILL.md:183` and the
installed `.claude/skills/hyalo-tidy/SKILL.md:180` ("Planned items where all
tasks are done"):

```bash
hyalo find --property status=planned --index --jq '.results | map(select((.tasks | length > 0) and ([.tasks[] | select(.status != "x")] | length) == 0)) | map(.file)'
```

`.tasks` is no longer in the default shape; `null | length` is 0 in jq, so
every file is dropped and the recipe reports "nothing to tidy", exit 0, no
warning. Measured: `status=completed` → 0 without `--fields tasks`, 243 with.
The iteration-252 consumer sweep missed it (the two `--view` recipes nearby
pin `fields` and still work). Fix: add `--fields tasks` in all three copies.
Every other documented recipe checked (`rule-knowledgebase.md`, `help.rs`,
`skill-hyalo.md`, `--broken-links --jq`) works.

### COH-6 / HELP-1: 16 short-help one-liners end mid-sentence (HIGH)

The 251 split inserted the blank line after the first *physical* line of the
old paragraph, so `-h` shows a fragment and `--help` shows the same fragment
followed by a blank line and the tail. Both pages are damaged.

| command | `-h` line (quoted) |
|---|---|
| `set`, `append` | `--validate   Validate new values against the schema from .hyalo.toml; reject writes that would` |
| `lint` | `--strict   Promote schema warnings to errors: "no 'type' property",` |
| `lint` | `--profile <NAME>   Overlay a named conformance profile for this invocation only (no` |
| `init` | `--profile <PROFILE>   Scaffold a preset vault flavour (okf, madr, skills, changelog) by` |
| `new` | `--file <FILE>   Vault-relative path for the new file (must not exist; parent dirs created if` |
| `changelog add` | `--wrap <COLS>   Wrap the entry to COLS columns, breaking on word boundaries and` |
| `links fix` | `--threshold   Minimum Jaro-Winkler stem similarity for a file to be considered a [default: 0.8]` |
| `links fix` | `--case-insensitive   Treat links that resolve only by case folding as resolved rather` |
| `links auto` | `--no-first-only`, `--exclude-target-glob`, `--no-warn-common-titles` (all cut) |
| `mv` | `[DEST]  Destination path — positional form (single-file mode only). Alias for --to:` |
| `mv` | `--allow-ambiguous … even when the stem is ambiguous` |
| `madr toc` | `[DIR]  ADR directory (vault-relative, must resolve inside the vault);` |
| `read` | `-s, --section <HEADING>   Extract section(s) by substring match (e.g. 'Tasks' matches 'Tasks [4/4]');` |
| `properties`, `tags` | `-g` ends `relative to --dir`; `-n` ends `Bare-group` |

Fix: rewrite each `help` as a complete sentence (concrete replacements are in
the agent notes; e.g. `--validate` → `Reject writes that would create lint
errors under the .hyalo.toml schema`; `--strict` → `Promote missing-type,
undeclared-property and date-format warnings to errors`). Add a
`check-help-drift` sub-check: no short `help` string may end in
`,;:` or a dangling word (`and by if a rather (no would to or`).

### COH-3 / COH-4: `find` headline and root `--help` JSON cookbook describe the pre-252 shape (HIGH)

- `hyalo --help` Commands and `find -h` line 1: *"returns file objects with
  metadata, structure, tasks, and links"*; `find --help` paragraph 1: *"…and
  optionally: frontmatter properties, tags, document sections, tasks, and
  links"*. Actual default keys: `file, lines, modified, properties, size,
  tags, title`. The FIELDS paragraph further down is correct.
- `hyalo --help` "# find — results is an array of file objects" shows
  `"properties": {"status": "draft", "title": "My Note"}, "tags", "sections",
  "tasks", "links"`. Actual: `title` is top-level and removed from
  `properties`; `size`/`lines` always present; sections/tasks/links absent.
  Same block "# read" shows `{file, content}`; actual carries `size, lines`.
  "# task read/toggle/set" shows one object; `--line 32,39,43` returns an
  array.

Fix: regenerate the shape block from a real run; rewrite the two headline
sentences to match the FIELDS paragraph.

### HELP-3: `views run -h` advertises `--filenames-only`/`--filenames0`, both ignored (HIGH)

`hyalo views run planned --filenames-only` prints the full JSON envelope.
The Output group on `views run -h` is copied from `find`; `views set -h`
also lists `--strict`, `--filenames-only`, `--filenames0`, `--limit` as
saveable. Wire them through or drop them from both pages.

### COH-10: `lint --help` example `hyalo lint --fix-rule HYALO001` fails (MEDIUM)

`error: the following required arguments were not provided: --fix`, exit 2.
The same recipe is in the repo `CLAUDE.md`. Fix example → `hyalo lint --fix
--fix-rule HYALO001`; short line → `With --fix, only autofix the specified
rule(s) (repeatable)`. Only real failure among 262 executed `--help` example
lines (the rest: placeholder paths, drift gates by design, or reference
syntax lines).

### FIND-3: non-string `title` disappears from the default shape entirely (MEDIUM)

`title: 42`, `title: [a, b]`, `title:` (null) → promoted `title: null` **and**
the raw key stripped from `properties`; 0.20.0 kept it under `properties`.
Reachable now only via `--fields properties` / `properties-typed`. DEC-252
says "no request can lose the value" — the default request does. Text prints
`title: (none)` for `42`. Fix: strip `title` from `properties` only when the
promoted title is non-null. Pre-existing, related: `--property title=42` → 0
(compares the display string, unlike every other key); `find --help` should
say so.

### FIND-1: `--fields` cannot name the always-on columns the help calls "fields" (MEDIUM)

`find --fields file,title` → `unknown field "file": valid fields are all,
properties, properties-typed, tags, sections (alias: outline), tasks, links,
backlinks, title`. But `find --help`, `views --help` and the text `fields:`
line all present `file/modified/size/lines` as fields. Accept-and-ignore
those names, or say in `--fields` help that they are unconditional.

### COH-9: "Every mutating command reports dry_run and skipped_count" is false (MEDIUM)

`hyalo --help` RESULTS CONVENTIONS vs measured `.results` keys: only
set/remove/append and the rename commands carry both. `mv` single has no
`skipped_count`; `mv` batch has `applied` not `dry_run`; `task toggle/set`
have neither; `links fix/auto`, `lint --fix`, `types set`, `lint-rules set`
lack `skipped_count`; okf/madr/changelog report `apply`; `new` has neither.
Soften the sentence or unify the envelope (bigger change).

### COH-15: every `--help` example line starts with U+00A0, so copy-paste fails (MEDIUM)

`find --help` example bytes: `c2a0 20 68 79 61 6c 6f …`. Pasting into zsh:
`zsh: command not found:  `. 200+ NBSPs across `--help` pages (by design, to
defeat clap indent stripping — `cli/args.rs:217,235-238`). `-h` pages use
plain spaces and paste fine. Use NBSP only on non-command indentation.

### COH-7: `find --help` PROJECTIONS prints control characters (MEDIUM)

"`--filenames0` … each path ends in  instead of ⏎" — the doc string embeds a
literal NUL and newline. Should read `\0` / `\n`.

### FIND-9: stale-index probe has a 1–2 s tolerance window (LOW, docs)

A directory 1 s newer than the index does not trip the warning; 2 s does.
Real users won't hit it; test scripts with `sleep 1` will (this session first
concluded UX-1 had regressed). Comment in `create-index --help` or use `>=`.

### FIND-8: `--fields all` costs ~20 % wall time on the indexed MDN vault even with `--limit 1` (LOW)

0.370 s default vs 0.442 s `--fields all` at `--limit 1`; suggests the heavy
per-entry fields are materialised before the limit. Candidate for iter-253's
neighbourhood.

## UX Issues

### COH-1 / HELP-4 / COH-2: three different "global options" lists, and `--dir` is listed nowhere (MEDIUM)

- Root `-h` lists 7 globals; root `--help` Options lists those + `--index-file`;
  root `--help` COMMAND REFERENCE lists 7 and says `--index-file` is
  per-subcommand; the 52 pointer lines list 6 (drop `--hints`,
  `--index-file`). `hyalo --index-file X tags` and `hyalo tags --hints` both
  work, so the pointer under-reports by two.
- `--dir` is named 19 times in `-h` one-liners ("relative to --dir") and in
  `--help` PATH RESOLUTION, but appears in no Options block anywhere. An agent
  cannot learn from help what `--dir` is or that `.hyalo.toml` sets it.

Fix: one source of truth; pointer → `Global: --dir --format --jq --count
--hints/--no-hints --site-prefix -q --index-file — see hyalo -h` (or drop
`--site-prefix` from the pointer and hide `--hints` from `-h`, HELP-9), and
list `-d, --dir <DIR>` on root `-h`. Or replace "relative to --dir" with
"vault-relative" everywhere.

### HELP-2: the shared `--file/--glob/--files-from` block is a 10-line paragraph on 5 pages (MEDIUM)

`read`, `task read/toggle/set`, `backlinks` never got a first-line split;
`--files-from` alone is a 5-line, 386 B paragraph ×5. The trio is 1.8 KB of
the 2.0 KB `task toggle -h` page; toggle-specific content is 5 lines. Use the
`find` wording verbatim; saves ≈1.3 KB per page, ≈6.5 KB total.

### HELP-5: `hyalo help <cmd>` is the 26 KB `--help`, not `-h` (MEDIUM)

`hyalo help find` = 26.5 KB / ≈6.6 k tokens; root `-h` only says
`hyalo <cmd> -h`. Either forward `help` to the short page or say
`hyalo help <cmd> = --help` on the root page. Also fixes HELP-13 (`help fnd`
has no did-you-mean; `fnd -h` does).

### HELP-7: `task --line` doesn't say file-line vs body-line; `read --lines` is body-relative (MEDIUM)

`task read --section Tasks` returns file-absolute lines (`:34`, `:38`,
frontmatter included); `read --lines` is "relative to body content". Sibling
commands, two conventions, only one states it. First toggle attempt from `-h`
used a body line → `line 21 is not a task`.

### HELP-6 / COH-11: `mv -h --property` lists a stale operator set (MEDIUM)

`K=V (eq), K!=V (neq), K>=V, K<=V, K>V, K<V, K (exists)`; `~=`, `!K` and
dot-paths work. `set/remove/append` say "Same syntax as find --property";
`mv` should too.

### COH-5: `read --help` never mentions `size`/`lines` or the large-body hint (MEDIUM)

JSON results carry `size`/`lines`; bodies over 8 KiB emit a `--lines 1:80`
hint — but only `find --help` says so. Plan 252 also claimed "text mode shows
size in the header line"; text mode has no header.

### FIND-7: `read`'s large-body hint arrives after the body has been paid for (LOW)

Threshold 8 KiB; the hint is on the `read` output itself. The decision point
is `size` on `find` — consider making the `find` per-file hint say
`read --lines 1:80` when `size > 8 KiB`.

### FIND-5: per-file `--fields all` hint fires on a truncated `--limit 1` (LOW)

`find --tag iteration --limit 1` (1 of 218) → per-file hint "See all
metadata for this file" — the description doesn't say what it adds. Match
the set-level wording ("Include the omitted fields (…)").

### HELP-8: `links fix -h` / `links auto -h` render in clap's two-line layout (MEDIUM)

`<MIN_CONFIDENCE>`, `<EXCLUDE_TARGET_GLOB>`, `<IGNORE_TARGET>`,
`<EXCLUDE_TITLE>` trip the next-line-help heuristic; ~35 % extra lines and a
different layout from the other 50 pages. `value_name = "N"/"GLOB"/"SUBSTR"/"TITLE"`.

### COH-16 / FIND-6: `--orphan`/`--dead-end` auto-include text disagrees on the same page (LOW)

Flag lines say `backlinks` / `links`; the `--fields` paragraph says both;
actual is both for both.

### COH-12 / COH-13: `--sort score` undocumented; "Wrong: `title=~/pat/`" is silently accepted (LOW)

The `--sort` error lists `score`; neither help page does. `find --help`
COMMON MISTAKES calls `=~` wrong, but `--property 'title=~/iter/'` returns
the same 32 as `~=`, no warning.

### COH-17: zero-result text output interleaves streams (LOW)

`No results for …` goes to stderr; the did-you-mean hint to stdout after two
blank lines — on a terminal the hint appears above the message it explains.

### COH-8: rustdoc intra-link leaks into `--help` (LOW)

`--index` long help on ~15 pages: "not the *list commands* of
[`crate::list_commands::LIST_COMMANDS`]". Move to a `//` comment.

### HELP-12 / minor wording drift (LOW)

`--glob <GLOB>` ×17 vs `<PATTERN>` ×5; `--file <FILE>` ×11 vs `<PATH>` ×5;
`--files-from <PATH>` vs `<PATH|->`; nine different `--dry-run` one-liners;
`behaviour` vs `behavior`; `(read-only)` suffix on 10 pages but not `find`,
`config`, `lint`, `*/list`; `summary -n` says `(default: 10) [default: 10]`;
`set --help` example `tags=[a,b,c]` unquoted (zsh glob error); three wrapped
`--jq` examples split across lines so the first line is an unterminated quote.

### HELP-14 / HELP-15: `summary -h` doesn't name its JSON keys; `find -h` leads with `--index` (LOW)

`summary --jq '.results.file_count'` → null (key is `.files`); an agent needs
`--jq '.results|keys'` first. `find -h` puts `--index/--index-file` above
Filters — the rarely-used pair is the first thing read.

### FIND-4: whitespace-only `title: "  "` wins over H1 (LOW)

Blank string should fall back to H1 like a missing key.

### UX-7: `find -- '--index && foo' --count` swallows `--count` as a file (LOW)

Everything after `--` is positional. The UX-6 tip should say to put flags
before the `--`.

### Root `-h` grouping and examples (LOW)

Grouping is right in substance. `create-index`/`drop-index` belong under
"write", not "config and scaffolds"; `properties tags … (bare = summary; …)`
is jargon. The 5 examples are 4× `find` + 1× `set`; agents' next moves after
find are `read --section` and `task toggle --line` — swap example 3 (overlaps
example 2) for those.

## What Worked Well

- All 52 `-h` pages under 3 KB; whole short surface ≈47 KB, 10× smaller
  than `--help`. `find -h` is the right shape: operators, sort keys and
  `--fields` values on one screen, Filters/Output grouping, 3 composed
  examples.
- Flag sets complete on both pages everywhere; pointer line byte-identical.
- Zero-result hints and typo tips are exactly what an agent needs.
- The compact shape generalises: 5–20× smaller on every KB; `--fields title`
  gives a flat ~230 B/item listing (a 50-item MDN call is 11 KB).
- Auto-include is complete and text/JSON agree in every combination tried;
  the text `fields:` line is the best discoverability aid of the release.
- `size`/`lines` byte-exact on disk, index and every edge case.
- 249's fixes, 250's CRLF convergence, and BM25 parity after
  `task toggle --index` all verified on scratch copies.
- Error envelopes uniform; exit codes match the documented 1/2 split in every
  case tried; `create-index --index-file` outside the vault refuses and names
  `--allow-outside-vault`.
- `find --help` FIELDS/SIZE, `views --help` FIELDS and `rule-knowledgebase.md`
  describe the 252 shape correctly — the drift is confined to two headline
  sentences, the root JSON cookbook, `read --help`, and one skill recipe.

## Performance

Best of 3, wall seconds, `--format json --no-hints`:

| command | own (disk, 427) | GH Docs (disk, 3.7K) | MDN (index, 14.4K) |
|---|---:|---:|---:|
| `find --limit 1` | 0.043 | 0.133 | 0.370 |
| `find --limit 1 --fields all` | 0.051 | 0.155 | 0.442 |
| `find "broken links"` | 0.201 | 1.004 | 0.376 |
| `find "broken links" --fields all` | 0.216 | 1.047 | 0.468 |
| `summary` | 0.080 | 0.424 | 0.751 |
| `find --property status=completed` | 0.047 | 0.135 | 0.389 |
| `find --limit 50` | 0.050 | 0.135 | 0.390 |
| `find --limit 50 --fields all` | 0.051 | 0.167 | 0.484 |

No regression vs the 0.20.0 report. Default is never slower than
`--fields all`; `--fields all` adds 10–25 % on the index (FIND-8), ≤20 ms on
disk. MDN index load ≈0.35 s regardless of query; `create-index` 4.5 s.

## Suggested follow-up (priority order)

1. FIND-2 tidy recipe `--fields tasks` (3 copies) — one-line fix, silent
   wrong result today.
2. COH-6 sentence-fragment sweep (16 flags) + a `check-help-drift`
   dangling-word guard.
3. COH-3/COH-4 headline + root JSON cookbook regenerated from a real run;
   COH-5 `read --help` size/lines.
4. HELP-3 `views run` Output flags; COH-10 `--fix-rule` example + CLAUDE.md.
5. COH-1/COH-2 single global-options list incl. `--dir`; HELP-2 shared
   file-selection block split.
6. FIND-3 keep raw `title` in `properties` when the promoted title is null.
