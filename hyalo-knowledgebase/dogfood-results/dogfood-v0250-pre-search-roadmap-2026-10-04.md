---
title: "Dogfood v0.25.0-pre — search roadmap, snapshot v4 and the review's correctness fixes"
type: research
date: 2026-10-04
status: active
tags: [dogfooding, search, index, links, lint, mv, schema, performance]
related:
  - "[[dogfood-results/dogfood-v0230-release-readiness-2026-09-13]]"
  - "[[reviews/codebase-review-2026-10-03]]"
  - "[[iterations/iteration-302-search-query-language]]"
  - "[[iterations/iteration-303-section-hits-and-facets]]"
  - "[[iterations/iteration-304-snapshot-v4-search]]"
  - "[[iterations/iteration-306-review-correctness-fixes]]"
  - "[[iterations/iteration-309-review-leftovers]]"
  - "[[iterations/iteration-310-serde-saphyr-1x]]"
  - "[[decision-log]]"
---

# Dogfood v0.25.0-pre — search roadmap, snapshot v4 and the review's correctness fixes

Binary: `hyalo 0.24.1 (752e619f07f9 2026-10-04)`, built from `main` and installed to PATH
(`cargo install --path crates/hyalo-cli --target-dir target --force`); the hash check passed
before any command ran. This is the unreleased 0.25.0 content: iterations 297–310 since the
13 September release-readiness check, i.e. the search roadmap (302–304), CI hygiene (305),
the correctness fixes from [[reviews/codebase-review-2026-10-03]] (306, 309), the JS API
follow-ups (307), docs drift (308) and the serde-saphyr 1.x move (310).

Four parallel explorer agents (≈70 min each) plus my own verification pass:

| Explorer | Corpora | Scope |
| --- | --- | --- |
| search | own KB (515), kepano (103), scratch `language: de` vault | iteration 302/303 query grammar, `terms`, sections, facets |
| index | MDN (14 375, scratch copy with v4 snapshot), GitHub Docs (3 710) | iteration 304 snapshot v4, incremental index, weights, parity, perf |
| correctness | 17 synthetic vaults, Obsidian Hub (6 519), kepano | review P1 fixes, DEC-348/350, `-q`, init, hints |
| links | GitHub Docs, VS Code docs, Obsidian Hub (scratch copies for writes) | iterations 297/298/301, `links fix`, anchors, help texts |

Every write ran in a `cp -R` scratch copy; `git status` on the real corpora is unchanged.
Findings marked **[verified]** were reproduced by me on the binary after the explorer
reported them.

## Headline

- **0 regressions.** Every P1 from the October review (emoji batch `mv`, `[[sub/rel-1.2]]`,
  gitignored named file, lint refusal envelope) is still fixed; DEC-348 code-span anchors and
  DEC-350 integer semantics hold; disk and `--index` are byte-identical on every query tried,
  including section mode, facets, slop and nested-YAML filters; incremental `create-index`
  scores equal a full rebuild.
- **1 HIGH, 10 MEDIUM, 9 LOW bugs**, the HIGH being `links fix --apply` writing GFM slugs
  into Obsidian wikilinks. Two MEDIUMs were found independently by two explorers
  (`summary.links.broken` excludes alias links; the did-you-mean hint is wrong).
- **Performance:** no > 2× regression. MDN ranked search is +23 % on disk and +31 % indexed
  against the 13 September numbers (BM25F + proximity cost); `summary` on disk is 1.15 s,
  within iteration 309's target. GitHub Docs `links fix` dry run is the outlier at 9.35 s.

## New feature verification

### Query grammar (iter 302) — WORKING

Precedence arithmetic is exact on the own KB: `snapshot mmap OR messagepack` = 20 =
`snapshot (mmap OR messagepack)`, `(snapshot mmap) OR messagepack` = 21,
`snapshot -(mmap OR messagepack)` = 138 = 158 − 20, `-(-snapshot)` = 158, lowercase `or`
works. `main()` equals `main`. `(snapshot`, `snapshot)`, `()`, `*` exit 1 with an
`invalid search query` envelope and a quoting hint; 300 nested parens say "nested deeper than
64 levels". `config*` finds 224 files; `a*` warns "matches 840 terms; only the 256 most
frequent are searched" and the warning survives `-q` **[verified]**. Field terms:
`title:index` = `title:Index` (42), `heading:goal` 256, `tag:iteration` 275 with the prefix
rule (`tag:project` matches `project/backend`, `tag:proj` does not), `path:DONE` 244, and a
field-only query returns a path-sorted list with `score: 0.0` and empty `matches`.
`std::fs`, `https://example.com` and `foo:bar` are plain words. A scratch `language: de` note
holding "Häusern" is found by `Haus`, `Häuser` and `haus` under an English default, on disk
and with `--index`. `hyalo terms` honours PREFIX, `--limit 0` (17 958 terms), `--count`,
`--jq` and `--index`.

### Did-you-mean (iter 302) — PARTIAL

The JSON `suggestions` array is right; the hints built from it are not. See UX-1.

### Section granularity and facets (iter 303) — WORKING

Hit shape `{file, section: {heading, level, line_start, line_end, path}, score, matches}`
with `heading: null, level: 0, path: []` for the preamble; nested `path: ["Intro","Deep"]`;
`--limit 5` counts sections; negation and field terms are decided per file
(`zzqqxx OR title:index` → 432 sections over 43 files); `--sort`, `--reverse`, `--fields`,
`-e` and a missing PATTERN exit 1 with envelopes; identical with `--index`. `--facet` counts
files over the full match set before `--limit` (105 total with `--limit 2`), reports missing
as `null`, flags the 50-bucket cap with `truncated: true` (`property:date`), rejects an
unknown spec with exit 1 naming the four forms, and combines with `--property`,
`--granularity section`, a field-only query and no PATTERN at all. Drill-down hints keep every
original flag.

### Snapshot v4 (iter 304) — WORKING, two honesty gaps

- **Old snapshot refused:** the pre-existing v2 (MDN) and v0 (GitHub Docs) indexes produce
  `warning: index format is older than this binary (index v2, binary v4); falling back to
  disk scan — re-run create-index`, exit 0, disk results. The warning is silenced by `-q`
  (BUG-7). `--index-file /nonexistent` exits 1 with the envelope and a `create-index --output`
  hint.
- **Incremental `create-index`:** fresh MDN 3.12 s → `{files_indexed: 14375, rebuilt: true,
  refreshed: 14375, reused: 0}`; no-op rerun `reused: 14375, refreshed: 0`; three edited
  files → `refreshed: 3`; two deletions → `removed: 2`; `--force` → `rebuilt: true`. Eight
  queries after the incremental run versus after `--force`: `{file, score}` identical.
- **In-memory stale repair:** two edited files without reindex → `note: 2 files changed on
  disk since the index was built (…); repaired in memory for this run (the index file is
  unchanged)`, both files returned, note survives `-q`, `.hyalo-index` size and mtime
  unchanged; four drifted files → three names plus `+1 more`; an unparsable drifted file is
  dropped with the same warning a disk scan gives. A same-second overwrite was detected.
- **Tokenizer v4:** on MDN `addEventListener` = `add-event-listener` = `add_event_listener`
  = `event listener add` = 2 164 notes, `listener` 2 364, `résumé` = `resume` 123, top-10
  scores identical disk versus index.
- **BM25F weights:** `numbat` in title / tag / heading / body scores 1.370 / 1.130 / 1.102 /
  0.832; `[search.weights] title = 1, body = 10` flips the order on disk and under `--index`
  without reindexing; `title = -3` and `proximity_bonus = -5` warn with the allowed range and
  keep defaults. `hyalo config` reports `results.search.{code_blocks, language, proximity_bonus,
  weights}`.
- **Slop and proximity:** `"ostrich emu"~2` matches only the close note, reversed order never
  matches; close pair scores 1.4995 versus far 1.0820, `proximity_bonus = 0` → 1.2853 / 1.0447;
  the snippet is the line holding both terms. Above 64 the slop clamps silently (UX-9).
- **`code_blocks = "skip"`:** fenced text drops out, inline code stays, on disk and indexed.
  A snapshot built under the other setting is correctly not used, but silently (BUG-8).
- **Parity:** eight MDN queries (section mode, `"event loop"~3`, three facets, `résumé`,
  identifier OR-chains) plus `terms` → `diff -r` identical; eight GitHub Docs structured
  filters on nested `versions.*` keys → byte-identical after `del(.hints)`.

### Review correctness fixes (iter 306/309/310) — STILL FIXED

| Finding | Status | Evidence |
| --- | --- | --- |
| F2 emoji batch `mv` | STILL FIXED | `mv --glob 'notes/*.md' --to archive/` moved `Emoji 🚀 Note.md`, `𝔘nicode 😀.md`, NFD `Café.md`; emoji directory, emoji `--to`, frontmatter `related: "[[Emoji 🚀 Note]]"` and self-anchor `[[Emoji 🚀 Note#Rocket 🚀]]` all rewritten and back-linked |
| F3 dotted stems | STILL FIXED | `[[sub/rel-1.2]]`, `[[sub/v2.0]]`, `[[a.b/c]]`, `[[sub/Note.MD]]` resolve; `[[sub/file.tar.gz]]` is an attachment; broken = 3 in `find --broken-links`, HYALO006, `summary.links.broken`, `links fix`; `backlinks sub/rel-1.2.md` lists 4 referrers |
| F5 gitignored named file | STILL FIXED | `.gitignore`, `.ignore`, `.git/info/exclude`, nested ignore with `!keep.md` all match `git status --ignored`; `summary.files.excluded: 7`; `--file MyNotes.md` returned under `--fields backlinks`, `links`, `all`; asymmetry documented (DEC-342) |
| F4 lint refusal | STILL FIXED | `lint -- missing.md` and lint under `[lint` unclosed TOML → exit 1, empty stdout, JSON envelope on stderr with a hint |
| DEC-348 code spans | STILL FIXED | `[[t#The \`code\` heading]]`, `#The code heading`, `#the-code-heading` resolve; `` `[[not a link]]` `` is no link; HYALO008 = find = summary = 3 |
| DEC-350 YAML 1.2 | STILL FIXED | `01234` → 1234, `0x1F` → 31, `0o17` → 15, `1e3` → 1000.0, `+7` → 7; `NaN`, `Infinity`, `.nan`, `.inf`, `yes`, `on`, `1:30` → string; `set` quotes `---`, `...`, `0X1F`, `1_000`, `0b101`, `yes`, `~`; `find --property 'a>1000'` matches the leading-zero integer. Three 1.1 forms still read as integers (BUG-14) |
| HYALO008 numbered headings | STILL FIXED | `## 1. Intro` ← `#1-intro`, `#1.-intro`, `#1. Intro`; `## 2.1 Sub Section` ← `#21-sub-section`; `#Browser_compatibility` and nested paths resolve; prefix `suggested_fragment` in all four tested shapes |
| `mv --on-conflict skip`, batch no-op, zero-result hint | STILL FIXED | skip reported on stderr and stdout, exit 0; `skipped_noop` in JSON; "`status` is set in 3 files, but never to that value: open (2), done (1)" |
| `init` CRLF | PARTIAL | `.claude/CLAUDE.md` stays pure CRLF through init / re-init / deinit; `AGENTS.md` does not (BUG-12) |
| enum without `values` refused | PARTIAL | refused at load with a clear message, but only `--strict` gates exit 1 (UX-2) |
| `-q` contract | PARTIAL | DEC-270 note, HYALO005 skip, lint UTF-8 skip silenced; the DEC-339 stale note survives as documented; no oversized-file limit exists to test (a 20 MiB note scans normally) |

### Link resolution and help (iter 297/298/301) — WORKING

Hidden-file markdown links (`[cfg](.gitignore)`, `[ci](.github/workflows/ci.yml)`,
`[up](../.gitignore)`) are `kind: attachment` and absent from every broken-link surface; a
missing hidden file stays broken; `.hidden` with only `.hidden.md` present is not inferred;
percent-encoding, `.` folding, `out_of_vault` and symlink refusal behave as specified;
disk/index parity holds on a 6 540-file Hub copy. HYALO008 defaults to warn, `--strict`
promotes, `lint-rules set HYALO008 --enabled false` round-trips through `.hyalo.toml`.
`links fix` on the Hub copy: `--apply` wrote 58 single-line, byte-minimal edits (8 alias,
48 case, 2 relocation), ambiguous links untouched, second run a no-op; `--apply --apply-fuzzy`
wrote exactly the one ≥ 0.8 proposal; `--min-confidence 0.5` wrote 7. Every `-h` global
line matches its `--help`; `--count` and `--index` are accepted exactly where advertised
(27 commands); single-target commands do not mention `--glob`; the largest `-h` is
`find` at 2 940 bytes. `views set --files-from -` is refused before any write.

## Bugs

### BUG-1: `links fix --apply` writes GFM slugs into Obsidian wikilinks (HIGH) [verified]

```text
## 3. Deploy Steps          (heading in "my note.md")
[[my note#Deploy Steps]]    (before)
[[my note#3-deploy-steps]]  (after hyalo links fix --apply)
```

hyalo resolves the result, Obsidian does not: Obsidian matches heading text, so only
`[[my note#3. Deploy Steps]]` works there. The markdown form `my%20note.md#3-deploy-steps`
is right. Expected: heading text for wikilinks, slug for markdown links. Impact: a repair
turns a hyalo-visible broken anchor into an Obsidian-visible one, in every vault where
`--apply` is used.

### BUG-2: `summary.links.broken` excludes bare `[[alias]]` links (MEDIUM) [verified, two explorers]

Obsidian Hub: `summary` → `broken: 154`; `find --broken-links --limit 0` → 162; HYALO006 →
162; `links fix` → broken 54 + ambiguous 100 = 154. The eight missing are `via: "alias"`
links, which DEC-308 declares broken. In a two-file vault (`lf.md` with `aliases: [Leah]`,
`talk.md` with `[[Leah]] and [[Nobody]]`) summary says `broken: 1` and lists `lf.md` as an
orphan, so the alias link is neither an edge nor broken. With `[links] aliases = true` all
surfaces agree. Contradicts DEC-318's single edge predicate.

### BUG-3: `mv` does not rewrite bare attachment links, and the resolver hides it (MEDIUM) [verified]

`hyalo mv e.md --to sub/e.md`: `[note](my%20note.md)` becomes `../my%20note.md`, but
`[img bare](img.png)`, `![embed](img.png)` and `[cfg](.gitignore)` are left untouched
(`total_links_updated: 2` of 4). Afterwards `find --file sub/e.md --fields links` still
reports `img.png -> img.png attachment`: a bare attachment name resolves vault-wide from any
directory, so hyalo never flags the stale link while GitHub or any relative renderer looks
for `sub/img.png`. Expected: rewrite to `../img.png` like the `.md` sibling.

### BUG-4: `read --lines` is body-relative while every printed line number is file-relative (MEDIUM) [verified]

`find "stale index" --granularity section` prints
`iterations/done/iteration-154-mv-index-patch.md … (lines 125-135)` and `--fields sections`
says `line: 125`, but `hyalo read <file> --lines 125:135` returns empty content, exit 0: the
file has a 16-line frontmatter and `--lines` slices the body. `read --format json` reports
`lines: 135` (the file count) although `read --help` says "body line count". The generated
hint silently compensates (`(lines 5-7)` hints `--lines 1:3`). Typing the displayed range by
hand reads the wrong lines, and `--lines 999:1000` is silently empty with exit 0. Expected:
one numbering base, and a notice when the range lies outside the body.

### BUG-5: `-term` as the first positional is parsed as a short flag (MEDIUM) [verified]

`hyalo find '-snapshot'` → clap reads `-s napshot`, a `--section` substring filter, so
"exclude snapshot" returns exactly the 25 files whose headings contain "snapshot", with only
the "`--section` matched more than one heading" warning. `hyalo find '-tag:iteration
snapshot'` → `invalid character ':' in tag name` (parsed as `-t ag:iteration`). With `--`
both work. `--help` documents the `--` requirement; nothing at runtime does. Expected: the
`--tag`/`--section` validation paths mention `--` when no PATTERN was given and the short
flag's value was concatenated.

### BUG-6: `--sort property:K` does not resolve dot-paths (MEDIUM) [verified]

GitHub Docs: `find --property 'versions.ghes~=/^[<>]/' --sort property:versions.ghes` and
the same with `--reverse` return the identical file-ordered list and warn
`no files have property 'versions.ghes' -- sort has no effect`, while the filter on the same
key matched 2 222 files and `--facet property:versions.ghes` buckets them. `--sort
property:contentType` (top-level) sorts and reverses correctly.

### BUG-7: old-snapshot refusal warning is silenced by `-q` (MEDIUM) [verified]

`hyalo find --index -q closures` on the real MDN tree (v2 snapshot): empty stderr, exit 0,
4.8 s disk scan. Without `-q` the "index format is older than this binary" warning prints.
The missing-index fallback (`failed to load index … falling back to disk scan`) does survive
`-q`. A scripted `-q` caller silently loses the index and the envelope has no source field.

### BUG-8: `code_blocks` mismatch between snapshot and config falls back to disk silently (MEDIUM)

Index built under the default, then `[search] code_blocks = "skip"`: `find --index
querySelector` returns the disk-with-skip answer in 4.68 s (indexed 0.74 s) with no message.
Reverse direction likewise. `summary --index` reports `.results.code_blocks: null`, so the
mismatch cannot be discovered from hyalo at all. Expected: a `-q`-proof warning naming both
settings, like the format-version refusal.

### BUG-9: HTML anchors are not recognised as anchor targets (MEDIUM)

`<a id="legacy-anchor"></a>`, `<a name="named-anchor"></a>`, `<h2 id="html-heading">`:
`[[d#legacy-anchor]]` and friends are `broken_anchor: true`, HYALO008 fires, `links fix`
defers them. Three of GitHub Docs' 374 broken anchors are satisfied by an `id=`/`name=`
attribute in the target. No help page mentions the limitation. Expected: resolve explicit
ids (GitHub/MDN semantics) or document the gap.

### BUG-10: `prefix*` stem fallback never fires when a typo stem shares the prefix (MEDIUM)

`find 'configuration*'` → 1 file, because `terms configuration` holds only
`configurationon` (a typo, 1 doc). `--help` promises "when the prefix as typed matches no
stem, its own stem is tried", but one typo stem counts as a match, so `configur` is never
searched; `config*` finds 224. Expected: union the typed-prefix expansion with the stem's.

### BUG-11: a literal backslash in a filename breaks every path-based command (LOW)

`notes/back\slash.md` (legal on macOS/Linux, vault `dir = "."`) is reported as
`notes/back/slash.md`; `--file 'notes/back\slash.md'` says file not found; `--glob 'notes/*.md'`
omits it; `mv --glob 'notes/**' --to archive/ --dry-run` and `set --glob 'notes/**'` abort
with `No such file or directory (os error 2)` and **exit 2**, which DEC-307 reserves for
internal errors; `lint` prints a skip note yet counts it in `files_checked`.

### BUG-12: `init --codex` writes the AGENTS.md managed block with LF into a CRLF file (LOW)

`printf '# Agents\r\n\r\nline.\r\n' > AGENTS.md; hyalo --dir kb init --codex` →
`file AGENTS.md` says "with CRLF, LF line terminators". `init --claude` against a CRLF
`.claude/CLAUDE.md` keeps pure CRLF. `deinit` restores AGENTS.md byte-identically.

### BUG-13: wikilink resolution does not fold Unicode normalization although the filesystem does (LOW)

APFS: with `Café NFD.md` (decomposed) on disk, `find --file 'Café NFD.md'` (precomposed)
and `mv` find the file because the OS folds, but `[[Café NFD]]` (NFC) → `path: null`;
only the byte-identical spelling resolves. Obsidian normalises to NFC. `links fix` has no
bucket for it. Impact: phantom broken links on macOS vaults synced from HFS+ or typed with a
different input method.

### BUG-14: YAML 1.1 numeric forms still parse as numbers under the stated 1.2 core contract (LOW)

`d: 1_000` → 1000, `p: 0X1F` → 31, `q: 0b101` → 5 in `--fields properties-typed`; YAML
1.2 core reads all three as strings. `set` already quotes them on write (DEC-350 names
`0X1F`), so `set --property x=0X1F` round-trips as text while the same token typed unquoted
reads back as 31 and `find --property p=31` matches it.

### BUG-15: `[x](/)` is broken although a root `index.md` exists (LOW)

GitHub Docs has 8 such links; `content/index.md` exists; scratch `[root](/)` → `path: null`.
`/dir` → `dir/index.md` works, so `/` → `index.md` is the consistent reading.

### BUG-16: zero-result diagnostic is wrong for dot-path keys (LOW)

`find --property 'versions.ghes=nothing'` → "No file has a `versions.ghes` property — list
the ones that exist", yet 2 222 files carry it. `'versions.ghes>=3.10'` and the top-level
`'contentType>=5'` print only `No results …` plus `-> hyalo properties summary`, never the
promised value list with counts (which the `=` form on a top-level key does print).

### BUG-17: `links fix` case-mismatch plan reports a target the rewrite does not write (LOW)

`[[sub/Note.MD]]` → plan `{"old_target": "sub/Note", "new_target": "sub/note.md"}`, applied
text `[[sub/note]]`. Only `emitted_target` is truthful. `--fields links` likewise reports
`target: "sub/note"` for `[[sub/note.md]]` although DEC-310 says `target` keeps what was written.

### BUG-18: `--facet property:status:extra` is accepted as key `status:extra` (LOW)

Yields one all-`null` bucket. Expected: reject like `property:` with a hint, or document that
colons are part of the key.

### BUG-19: `lint` and `find` disagree on whether a non-UTF-8 note is readable (LOW)

`bin.md` with `\xff\xfe\x00` in the body: `find` lists it and `summary` counts it with
`skipped: 0`; `lint` prints "skipping ./bin.md (invalid utf-8 sequence …)" and adds a
"could not read file" row.

## UX issues

### UX-1: did-you-mean hints are wrong more often than right (MEDIUM) [verified, three observers]

Three defects compound: (a) `find snapshto` lists "snapshoton (2 docs), snapshots (1 doc),
snapshot (158 docs)" and the hint runs `find -- snapshoton`, the typo stem — the order is
neither edit distance nor docs (all three are Levenshtein 2; `snapshot` is Damerau 1);
(b) with two misspelled words (`ostrch kangroo`, `snapshoot incrementl`) the corrected hint
fixes only the first term, so both hinted commands return 0 while `suggestions` carries
both corrections; (c) the `Try OR instead of AND` hint is offered when neither word occurs
in any document, so it cannot help. Tie-break by docs, correct every term, and offer `OR`
only when at least one term has documents. A misspelled word inside a phrase (`"stale
indx"`) yields `suggestions: null`.

### UX-2: a schema error is not a gate refusal while `config` says `malformed: true` (MEDIUM)

`enum` without `values`: `hyalo config` → `malformed: true` plus `schema_error`, yet plain
`lint` exits 0 with a warn-level `SCHEMA` row, `views run open` runs the view, and
`views run foo` answers "unknown view" with the diagnostic only on stderr. Only `lint
--strict` and `find --strict` exit 1; a TOML syntax error refuses all four. Either
`malformed` should mean refusal everywhere, or the schema case should not be reported under it.

### UX-3: `--apply-fuzzy` without `--apply` prints "written" during a dry run (MEDIUM) [verified]

Hub: `links fix --apply-fuzzy --format text` → `1 at or above the confidence floor 0.8
(written — --apply-fuzzy)` followed by `Applied: no (dry run)`; JSON `dry_run: true,
applied: false, fuzzy_applied: true`. `-h` says "Apply low-confidence fixes too" and never
says `--apply` is still required; `fuzzy_applied` reads as an outcome.

### UX-4: `lint --fix` rewrites tabs inside Go code fences (MEDIUM, by design)

GitHub Docs: `lint --fix --fix-rule MD010` on `copilot/how-tos/copilot-sdk/getting-started.md`
changed 215 lines, every hunk inside a `golang` fence (gofmt tabs → spaces); the corpus dry
run plans 579 MD010 edits, 39 of 42 sampled inside fences. DEC-316 keeps MD010 checking
fences on purpose and points at `<!-- markdownlint-disable no-hard-tabs -->`; a user running
`--fix` on a corpus with Go or Makefile samples still corrupts code. The dry run could at
least mark proposals as "inside code block" so the user sees it before applying.

### UX-5: the 50-item caps are invisible to scripts and undiscoverable (LOW, three explorers)

`find --broken-links --format json` on GitHub Docs: `total: 1904`, 50 results, no
`truncated` key; the CLAUDE.md broken-link, histogram and missing-images recipes carry no
`--limit 0` and `--jq` strips the `--limit 0` hint, so each silently reports a sample (150 of
7 788 lines). `lint` JSON caps `files` at 50 with `files_truncated: true`, so per-rule sums
disagree with `violations` (112 versus 177 on the Hub) and HYALO005 vanishes entirely on
kepano; the lifting flag is `-n 0`, which no hint names, and `--max-files` does not exist.
Fix the shipped recipes to pass `--limit 0` and add a `truncated` field to `find`.

### UX-6: malformed query input is silently accepted (LOW)

`a "b` → 171 results; `a OR` and `OR a` → everything (only bare `OR` warns); `*foo` → 76;
`sn*p` → 0 with a `terms snp` hint; `"stale index"~abc`, `~-1`, `~ 3` treat the suffix as a
word; `~65` through `~99999999999999999999` clamp to 64 with no warning; `-` alone → 0
results with a `properties summary` hint. Warn like `OR` does or reject like `()`.

### UX-7: anchor-repair surfaces disagree with each other (LOW)

For `[[note#Intro]]` with headings `## 1. Intro` and `## Intro-duction`, `find --broken-links`
says `did you mean "#Intro-duction"?` while `links fix` plans `#Intro → #1-intro`. Every
`deferred_anchor_fixes` entry carries the same fixed reason ("no unique exact numbered-heading
match; similar headings are advisory only") and never surfaces the `suggested_fragment` that
`find` shows. The text summary says "Fixable: 0" while the hint says "Apply 2 fixes".

### UX-8: hints missing where a user expects one (LOW)

`new --dry-run` prints the scaffold with `hints: []` (no `=> hyalo new … [writes]`);
`mv --on-conflict skip` dry run has `hints: []` although a move is planned; `links fix` dry
run offers only `-> find --broken-links` even with 8 alias, 48 case and 6 anchor proposals,
so the `=> --apply [writes]` channel is never shown; `lint --rule-prefix HYALO` hints
`types list`; `summary` hints `create-index # create an index` while a 10 MB `.hyalo-index`
already exists and 19 files drifted; the corrected-query hint drops `--granularity section`.

### UX-9: `--site-prefix` is a no-op on GitHub Docs and the documented warning never fires (LOW)

`""`, `content` and `zzz` all give broken 7 414 because root-absolute links fall back to the
vault root when the prefix strips nothing; `find --help` says `links fix` warns when the
prefix stripped 0 of N, and `links fix --site-prefix zzz` prints nothing. VS Code (`docs` →
232, `""` → 2 755) shows the prefix does work when a real segment must be stripped.

### UX-10: smaller items (LOW)

- `title:(index OR snapshot)` fails with "')' without a matching '('" because `(` was
  swallowed into the field operand; say a field term cannot take a group.
- A pure-negative query (`-- '-snapshot'`) says "No results" and hints `properties summary`
  instead of "a query needs at least one positive term".
- Section-mode text prints snippet lines out of order (`line 145`, `149`, `147`).
- `summary --index` on a refused snapshot reports `index_format_version: null`; only stderr
  carries `index v2`.
- `terms work --index` on GitHub Docs lists joined slug wholes (`workflowsyntaxforgithubact`,
  66 docs) that nobody types and that occupy slots in the 256-term prefix cap.
- `create-index --force` (4.07 s) is slower than `rm .hyalo-index && create-index` (3.12 s);
  a no-op `create-index` still rewrites 141 MB (1.8–4.6 s).
- `links fix --apply --min-confidence 0.5` rewrote `[[<plugin-id>]]` (a placeholder) at 0.56;
  `<…>` targets belong in the `templated` bucket with `{{…}}`.
- `lint-rules -h` and `types -h` advertise `--count` that only their `list` subcommand accepts.
- HYALO005 on kepano's 28 Templater templates says "line 6 column 11: unexpected end of
  input" without naming the key (`created: {{date}}`); the skipped files are also invisible
  to `find -e '\{\{'`.
- `.gitignore` is honoured only inside a git repository while `.ignore` always is — ripgrep's
  rule, but the claims paragraph says "the way Git does" without the `.git` condition.
- `set --help` says a date needs a schema to be typed, but `set --property x=2024-01-01`
  writes it unquoted and it reads back as `date` without one (doc drift).

## What worked well

- Every surface that is supposed to agree does, except the alias case above: F3 across five
  commands, orphans/dead-ends between `summary` and `find` on all five corpora, HYALO008 =
  `find` = `summary` on anchors, disk = index on ~30 distinct queries.
- Malformed-query envelopes name the exact problem and hint quoting; the `[writes]` tag on
  `=>` hints is clear and every `->` hint an explorer ran was syntactically runnable,
  including `--site-prefix ''`, `--index` threading, and emoji paths quoted correctly.
- The zero-result `--property` hint ("`status` is set in 3 files, but never to that value:
  open (2), done (1)") is exactly the right answer on top-level keys.
- Stale-repair note names count and files, survives `-q`, never writes the snapshot, and
  mirrors the disk-scan warning for an unparsable drifted file.
- `set` quotes every token the parser would otherwise coerce and all of them round-trip.
- `mv` ambiguity guard on the Hub: moving `plugins-galore.md` lists five `skipped_ambiguous`
  with both candidates and the source line; the 2 190-backlink note moves in 1.3 s.
- `links fix --apply` on the Hub copy is byte-minimal (`git diff --numstat` 57/57) and
  idempotent.
- `hyalo lint --strict` on the own KB exits 0, as iteration 309 promised.

## Performance

All best-of-3 wall times, warm cache, same Mac as the 13 September report.

| Operation | Prior | Now | Note |
| --- | ---: | ---: | --- |
| Own KB `summary` | 0.060 s | 0.079 s | |
| Own KB ranked search (disk / `--index`) | 0.271 s | 0.36 / 0.165 s | |
| Own KB `lint --strict` | 0.190 s | 0.25 s | |
| Own KB `terms` / section query / 3 facets | – | 0.18 / 0.38 / 0.35 s | |
| MDN ranked search disk | 3.899 s | 4.814 s | +23 % |
| MDN ranked search `--index` | 0.565 s | 0.740 s | +31 % |
| MDN title filter disk / `--index` | 0.586 / 0.301 s | 0.641 / 0.352 s | |
| MDN `summary` disk | ≤ 1.55 s target | 1.146 s | within target |
| MDN `summary --index` | – | 0.473 s | |
| MDN fresh `create-index` | 2.857 s | 3.12 s | snapshot 141 MB vs 124 MB |
| MDN `create-index --force` / no-op | – | 4.07 / 1.85 s | |
| MDN `terms` disk / `--index` | – | 2.17 / 0.73 s | |
| MDN section query disk / `--index` | – | 4.74 / 0.73 s | |
| MDN 300-word query disk / `--index` | – | 5.75 / 0.75 s | |
| GitHub Docs `summary` / `create-index` | – | 0.30 / 0.91 s | |
| GitHub Docs `links fix` dry run | – | 9.35 s | 7 414 broken targets → 5 478 fuzzy candidates |
| GitHub Docs `lint --fix --dry-run` | – | 1.56 s | |
| Hub `summary` / `links fix` / strict HYALO lint | – | 0.38 / 0.52 / 1.04 s | |
| Hub ranked search disk / `--index-file` | – | 1.49 / 0.30 s | |
| Hub `mv --dry-run` 2 190-backlink note | – | 1.31 s | |
| kepano `summary` / `lint --strict` | – | 0.04 / 0.06 s | |

No operation exceeds 2× its prior sample. The MDN ranked-search increase is the price of
BM25F field scoring plus the proximity pass over the top 200 candidates; the index-versus-disk
ratio on the same corpus stays about 7×.

## Recommended follow-ups

1. **Anchor repair and alias parity** (BUG-1, BUG-2, UX-7, BUG-17): write heading text into
   wikilinks, make `summary.links` use the same predicate as `find`/HYALO006 for alias links,
   and surface `suggested_fragment` in `deferred_anchor_fixes`.
2. **Honesty of `--index`** (BUG-7, BUG-8, UX-10): `-q`-proof refusal warnings for both the
   format version and the `code_blocks` mismatch, and `index_format_version`/`code_blocks`
   in `summary --index` for the refused snapshot.
3. **Did-you-mean and query diagnostics** (UX-1, UX-6, BUG-10): tie-break candidates by docs,
   correct every misspelled term in the hint, offer `OR` only when it can match, warn on
   clamped slop and dangling operators, union the prefix expansion with its stem.
4. **`mv` attachment links and line-number base** (BUG-3, BUG-4, BUG-5): rewrite bare
   attachment links on move, make `read --lines` file-relative or say which base it uses, and
   mention `--` when a short flag swallowed a leading-dash pattern.
5. **Nested-key sort and zero-result wording** (BUG-6, BUG-16), **HTML anchors** (BUG-9,
   decide-or-document), the shipped recipes' `--limit 0` and a `truncated` field (UX-5).
6. Backlog the LOWs: backslash filenames with exit 2, `init --codex` CRLF, NFC/NFD folding,
   YAML 1.1 numeric leftovers, `[x](/)`, placeholder fuzzy candidates, `--force` cost.

Scratch artefacts for reproduction (session scratchpad, not committed):
`explorer-correctness/v1-emoji … v13-nfc`, `explorer-links/hv` and `hub` (git history of each
apply), `explorer-index/` MDN and GitHub Docs copies with v4 snapshots, `verify-links/` for
BUG-1 and BUG-3.
