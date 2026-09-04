---
title: "Dogfood v0.22.0 — after iterations 261–270: regression sweep, write atomicity, autofix and site_prefix corruption"
type: research
date: 2026-09-04
status: active
tags:
  - dogfooding
  - obsidian
  - links
  - lint
  - schema
  - mutations
  - index
  - performance
related:
  - "[[dogfood-results/dogfood-v0220-obsidian-vaults]]"
  - "[[iterations/iteration-261-link-resolution-obsidian-compat]]"
  - "[[iterations/iteration-262-frontmatter-wikilinks-first-class]]"
  - "[[iterations/iteration-263-lint-autofix-obsidian-safety]]"
  - "[[iterations/iteration-264-find-sort-filter-consistency]]"
  - "[[iterations/iteration-265-scan-exclude-and-skipped-files]]"
  - "[[iterations/iteration-266-properties-tags-schema-mutations]]"
  - "[[iterations/iteration-267-help-hints-text-polish]]"
  - "[[iterations/iteration-268-object-list-schema-type]]"
  - "[[iterations/iteration-269-mv-frontmatter-link-scan-gap]]"
  - "[[iterations/iteration-270-schema-write-semantics]]"
  - "[[backlog/mv-destination-path-resolved-vault-relative]]"
---

# Dogfood v0.22.0 — after iterations 261–270

Binary `hyalo 0.22.0 (18fca3f975db 2026-09-04)`, built from `main` after PR #314. Four parallel
explorers, each with its own testbed and a scratch copy for every mutation; originals were never
written to (obsidian-hub's 14 pre-existing uncommitted edits verified intact afterwards).

| Testbed | Files | Role |
|---|---|---|
| `hyalo-knowledgebase/` (own KB) | 453 | find/config/schema/mutation surface, help vs behaviour |
| `../obsidian-hub` | 6540 | regression of every finding in the previous report, link kinds, perf |
| `../kepano-obsidian` | 103 | property-rich Obsidian regression, `[scan] exclude`, `mv` |
| `../mdn/files/en-us` | 14375 | index parity, link kinds at scale, `site_prefix`, perf |
| `../docs/content` (GitHub Docs) | 3710 | Liquid-heavy autofix safety, nested YAML dot-paths |
| synthetic `edge-*` vaults | tiny | parser torture, byte preservation, concurrency, corrupt index, exit codes |

Headline: the 261–270 batch did what it claimed. **41 of 45** items from the previous report are
fixed, 2 partially, 1 never scheduled, **0 regressed**. The Hub's broken-link count went
3149 → 163, HYALO006 2897 → 109, kepano `.base` false positives 53 → 0, orphans 25 → 1. Index
parity is byte-identical on all nine MDN commands including BM25 scores, and no command is more
than 1.5× its baseline.

The new round found **27 bugs (5 HIGH, 10 MEDIUM, 12 LOW)** and 25 UX issues. Four of the five
HIGH ones can silently corrupt a vault with exit 0, and none of them is in code the batch touched:
they are older gaps the batch's new testbeds and the adversarial pass exposed. Zero panics in
roughly 150 hostile invocations with `RUST_BACKTRACE=1`, including truncated, zeroed and
byte-flipped index files.

## Regression of the previous report

| Range | Result |
|---|---|
| BUG-1 … BUG-13, BUG-15 … BUG-25 | STILL FIXED, each re-run with the original command on the original vault |
| BUG-14 (invalid-UTF-8 file in index) | PARTIALLY: `create-index` now reports `warnings: 1` and names the file, but `find --file bad.md --index` still lists it |
| UX-1 … UX-7, UX-9 … UX-18, COH-13 | STILL FIXED |
| UX-8 (nonsense fuzzy candidates) | PARTIALLY: `.base` and 0.0-confidence candidates are gone, but alias-backed names (`Cat`, `Leah`, `jamesb`) still score above the 0.8 apply floor against unrelated notes — see BUG-6 |
| COH-17 (zero-result stream ordering) | NOT FIXED, never scheduled; and the hint text is wrong (BUG-17) |

Selected numbers, Hub unless stated: `summary.links.broken` 163 (AC ≤ 300) = 64 truly broken +
99 ambiguous short-form links; `links fix` reports `case_mismatches 48` outside `broken`,
`fuzzy_below_floor 18` held separately; `lint --fix --dry-run` proposals 24363 → 24044 with MD018
0, MD001 0, MD042 empty-link 0, and `Themes/Retroma.md:65` now wraps to `<url>` without swallowing
`<br`; `links auto --dry-run` 7014 matches with 11496 mentions held back by the stop-list; kepano
`summary` stderr is exactly one line. DEC-281 verified on kepano: `type: ["[[Authors]]"]` no
longer yields `expected string` there — but see BUG-5 for what happens once a schema exists.

## Bugs found

### BUG-1: Concurrent `set`/`append` on one file loses updates while every process exits 0 (HIGH)

```text
printf -- '---\ntitle: P3\n---\nbody\n' > p3.md
for i in $(seq 1 20); do ( hyalo set p3.md --property k$i=v$i -q --format json >/dev/null 2>&1; echo "k$i $?" >> ex.txt ) & done; wait
grep -c ' 0$' ex.txt      # 16–19 report success
grep -c '^k[0-9]*:' p3.md # 2–3 keys actually present
```

Two-way version: 9 of 10 rounds of `set a=1` ∥ `set b=1` lose one key whose process exited 0;
20 parallel `append` to a list leaves 3 items. The TOCTOU fingerprint in
`hyalo-core/src/frontmatter/mod.rs` is `(mtime, size)`: both writers read the same fingerprint
before either renames, the check passes, and the temp-file rename clobbers the other. Iteration
122 listed this window as known. Impact: silent data loss under any parallel automation (`xargs
-P`, two agents on one vault) with false success. Fix direction: content hash or an advisory lock
around read-modify-rename; APFS mtime is nanosecond, so this is structural, not granularity.

### BUG-2: An indented `  ---` inside a block scalar is taken as the closing fence, and `set` writes through it (HIGH)

```text
printf -- '---\ntitle: Ind\nk: |-\n  a\n  ---\n  b\nafter: 1\n---\nREALBODY\n' > ind.md
hyalo read ind.md --frontmatter --jq '.results.frontmatter'   # {"k":"a","title":"Ind"}  (after: lost)
hyalo set ind.md --property z=1; diff ind.orig ind.md
# 5c5,6   <   ---   >  z: 1   > ---
```

The `  ---` line inside the literal scalar is replaced by the new key plus a fence; `  b` and
`after: 1` become body text; `lint` reports nothing. hyalo manufactures the trigger itself:
`hyalo set ml2.md --property "k=$(printf 'a\n---\nb')"` writes `k: |-` with an indented `---`,
and `find --file ml2.md` then reads `k: "a"`. `is_closing_delimiter` in `frontmatter/parse.rs`
documents `line.trim() == "---"` as deliberate leniency, but no DEC covers it and both YAML and
Obsidian close only at column 0. Fix: strict column-0 close like the opener, or at least make the
emitter quote or refuse any scalar containing a line that trims to `---`.

### BUG-3: `lint --fix` MD031 inserts a blank line inside an unterminated fenced code block (HIGH)

```text
printf -- '---\ntitle: t\n---\n\n# T\n\nIntro.\n\n```yaml\n  - uses: x\n  - name: y\n' > unterm.md
hyalo lint --fix --file unterm.md     # fixed MD031 line 9
sed -n '9,11p' unterm.md              # ```yaml / <blank> / "  - uses: x"
```

A fence that never closes runs to EOF; hyalo treats the opener as a closer and "adds a blank line
after it", inside the sample. Real hit: `../docs/content/actions/tutorials/build-and-test-code/rust.md`
(fence opened at line 169 never closed) gets a blank inserted before its first `- uses:` line. Six
GitHub Docs files have an odd fence count; MDN has none. markdownlint reports nothing at an
unterminated opener. Iterations 263 and 269 did not cover this shape.

### BUG-4: `links fix --apply` on a `site_prefix` vault appends `/index` to every case-folded link, and the dry-run shows a different target (HIGH)

On a copy of `../mdn/files/en-us/web/css` with `site_prefix = "en-US/docs/Web/CSS"`:

```text
hyalo links fix --dry-run --jq '.results.case_mismatch_fixes[0]'
# old_target "/en-US/docs/Web/CSS/Guides/Anchor_positioning", new_target "guides/anchor_positioning/index.md"
hyalo links fix --apply     # case_mismatches 5096, applied 5096, 1049 files changed
git diff -U0 | head
# -[…](/en-US/docs/Web/CSS/Guides/Anchor_positioning)
# +[…](/en-US/docs/Web/CSS/guides/anchor_positioning/index)
```

Three problems: the written URL carries a trailing `/index` nobody publishes; the dry-run shows a
vault-relative path while apply writes the site-absolute form, so the preview does not describe
the rewrite; and DEC-267 calls `link-case-mismatch` cosmetic while it rewrote 5096 links in a
corpus whose URL convention is Title-case over lowercase folders. `--case-insensitive` suppresses
the plans, but plain `--apply` is the documented default. Expected: a directory-index link that
resolved via `site_prefix` keeps its incoming form (at most a case change), and the dry-run must
print the exact string that will be written.

### BUG-5: A one-element list `type:` binds to its schema and is then rejected as `expected string` (HIGH)

```text
# .hyalo.toml declares type "iteration"
printf -- '---\ntitle: L\ntype: ["[[iteration]]"]\ndate: 2026-09-04\nstatus: planned\nbranch: iter-1/x\ntags: [a]\n---\n' > l.md
hyalo lint --file l.md
#   error  SCHEMA  line 1  property "type" expected string, got ["[[iteration]]"]
```

Binding works (a variant with `status: bogus` gets the iteration-specific enum error), but the
implicit `type: string` constraint every declared type carries then rejects the list value. Plain
`type: "[[iteration]]"` lints clean, and kepano is clean only because it has no `.hyalo.toml`
schema. DEC-281's motivating case, Obsidian's property editor writing `type: ["[[Authors]]"]`,
errors on every file the moment a schema is declared. `types set --help` promises the opposite.

### BUG-6: Frontmatter `aliases:` are not link targets, so alias links are broken and `--apply-fuzzy` would rewrite them to the wrong note (MEDIUM)

```text
printf -- '---\ntitle: Leah Ferguson\naliases:\n- Leah\n---\n' > 'al/Leah Ferguson.md'
printf -- 'see [[Leah]]\n' > al/src.md
hyalo find --file al/src.md --fields links --jq '.results[0].links[0]|{target,path}'   # path: null
```

Hub audit: of 47 distinct genuinely-broken targets, 7 (`leah`, `boninall`, `tim hor`, …; 9
occurrences) are declared aliases of an existing note, and `links fix --apply-fuzzy` would
rewrite `Leah → Lewuathe.md` (0.87), `Cat → CatMuse.md`, `jamesb → jamesgreenblue.md`. The Hub
has 5489 distinct aliases. Obsidian resolves all of them; no DEC covers alias resolution.
Expected: resolve `[[X]]` against `aliases:` (unique alias, else ambiguous), and never
fuzzy-match a target that is some note's alias.

### BUG-7: `mv` rewrites ambiguous frontmatter wikilinks that it refuses to rewrite in the body (MEDIUM)

```text
mkdir x; printf -- '---\ntitle: A\n---\n' > a.md; printf -- '---\ntitle: XA\n---\n' > x/a.md
printf -- '---\ntitle: C\nrelated: "[[a]]"\nrel2: [[a|al]]\n---\nbody [[a]] and [[a|al]]\n' > c.md
hyalo mv a.md z.md
# note: skipped ambiguous link [[a]] at c.md:6 (twice)  …  files updated: 1, links updated: 2
```

`c.md` ends with `related: "[[z]]"` and body `[[a]]`: half retargeted, no warning for the
frontmatter side. DEC-269 made frontmatter links graph edges; the DEC-288 ambiguity guard must
cover them too. The Hub, with many `related:` lists and 99 ambiguous stems, will hit this.

### BUG-8: `[text](#fragment)` same-page links are reported as `kind: "wikilink"` with `target: ""` and `label: null` (MEDIUM)

MDN has no wikilinks, yet the whole-vault histogram says `wikilink: 2822`; GitHub Docs `1552`
(its README TOC). `- [Predefined fallback options](#predefined_fallback_options)` → `{"kind":
"wikilink","label":null,"target":"","fragment":"predefined_fallback_options","broken_anchor":true}`.
Expected `kind: "markdown"` with the link text as label. Disk and index agree, so it is
extraction, not the snapshot.

### BUG-9: `find --file X --broken-links` and `--glob … --broken-links` drop `broken_anchor` and `suggested_fragment` (MEDIUM)

Vault-wide `find --broken-links` prints `line 22: "Target#Sec" → "Target.md" (broken anchor) — did
you mean "#Section One"?`; `find --file Source.md --broken-links` prints the same link without the
annotation and the JSON lacks both keys; the positional form `find Source.md --broken-links` keeps
them. The DEC-268 suggestion is invisible in exactly the per-file workflow `links fix` points at.

### BUG-10: `find --file <unparsable>` returns an empty result with exit 0 (MEDIUM)

```text
printf -- '---\ntitle: Dup\ntitle: Dup2\n---\n' > dup.md
hyalo find --file dup.md --format json   # results: [], total: 0, exit 0, one skip warning
hyalo set dup.md --property x=1          # error … unparseable frontmatter; nothing was modified, exit 1
```

DEC-277 cites iteration 204: naming one unparsable file is an error, not a warning. The DEC-278
skip collector turned the named-target error path into a counted skip; `set` still refuses, so
`find` is the odd one out.

### BUG-11: `find --index --file <file not in the snapshot>` returns an empty result, `files_missing: 0`, exit 0 (MEDIUM)

After `create-index`, `printf … > brand-new.md; hyalo find --index --file brand-new.md` →
`results: []`, exit 0, while the disk run finds it. Same four levels deep. A file that is in the
snapshot is stat-refreshed correctly (DEC-280). Expected: upsert the named file (one stat, one
parse) or report it under `files_missing`; never an empty success.

### BUG-12: The stale-index warning does not fire for in-place edits (MEDIUM)

`create-index; sleep 1.1; printf … > n2.md` (overwrite) then `find --index --property
status=final` → `n2.md` missing, no warning, exit 0; creating a new file does warn. DEC-280's
directory-mtime probe cannot see an in-place overwrite on APFS, which is the most common edit
(Obsidian saving a note). The index already stores per-file mtimes; folding the newest into the
probe would close it cheaply.

### BUG-13: `properties rename --to ''` writes an empty key (MEDIUM)

`hyalo properties rename --from title --to ''` → exit 0, every file now has `"": Note 2` and its
title silently falls back to the filename stem. `types set ''` and `--property '=v'` are rejected;
this should be too.

### BUG-14: `mv` destination is not vault-prefix-stripped and nests the vault directory (MEDIUM, characterised)

With `dir = "kb"` from the parent directory, `hyalo mv kb/a.md kb/sub/a.md`, `--file kb/a.md --to
kb/sub/a.md` and `--glob a.md --to kb/sub/ --apply` all move to `kb/kb/sub/a.md`; the source side
strips the prefix, the destination does not, and link rewrites follow the nested path. `--to
kb/sub/` yields the hint `did you mean kb/sub/.md?`. Already filed as
[[backlog/mv-destination-path-resolved-vault-relative]]; all three forms are affected.

### BUG-15: A frontmatter flow list beginning `[[[` mis-captures the link target, so `mv` leaves it dangling (MEDIUM)

Real file, own KB: `research/agent-ergonomics-ralph-loop-port-2026-08-24.md` line 7 is
`related: [[[iterations/iteration-206-links-perf-profiling]], [[research/…]], …]`. The raw-text
scanner captures `[iterations/iteration-206-links-perf-profiling` (leading `[`), HYALO006 reports
it broken, and `mv iterations/iteration-206-… iterations/done/…` skips the file. The file is
mis-authored (YAML reads a nested list) but Obsidian renders the link, and the scanner already
ignores YAML structure by design. Only one such line exists in the KB; worth fixing the file too.

### BUG-16: A wikilink target may contain `]` and `[[` (LOW)

`see [[Leah] here and [[Target] and [Target]] and [[ ]]` yields one link with target
`Leah] here and [[Target] and [Target` and one with target `" "`. Real Hub occurrence:
`Obsidian Community Talks.md:64` ends in `[[Leah]`. The scanner should stop at `]` or `[[` and
treat the opener as prose.

### BUG-17: The zero-result hint claims the property does not exist when it does (LOW)

kepano: `find --property status=zzz` → `# No file has a \`status\` property — list the ones that
exist`, while `find --property status --count` → 5. The hint should separate "no file has the
key" from "no file has that value", and could list the existing values.

### BUG-18: `summary --index` reports `excluded: 0` where the disk scan reports 52 (LOW)

kepano copy with `[scan] exclude = ["Templates/**"]`: disk `{total:51, excluded:52}`, index
`{total:51, excluded:0}`. DEC-277 filters at snapshot load but drops the count. Every other
disk-vs-index comparison was identical.

### BUG-19: `[links] case_insensitive = "false"` returns a non-canonical path and is invisible in `hyalo config` (LOW)

On macOS, `[[categories/books]]` reports `path: "categories/books.md"` (no such vault path; the
file is `Categories/Books.md`) and `[[BOOKS]]` still resolves, because the literal probe hits the
case-insensitive filesystem. `config` prints no `case_insensitive` key although the skill
documents it. `"false"` should mean exact match or the docs should say filesystem-dependent.

### BUG-20: `lint --rule X` leaks the HYALO005 parse error into the filtered result and `--count` (LOW)

Hub `lint --rule MD018 --count` → 1, the hit being `could not parse frontmatter` for the Daily
Log file; the same file appears under `--rule MD042` and `--rule HYALO006`; kepano
`--rule HYALO006` lists 28 template parse errors. Scripts counting one rule get parse errors added.

### BUG-21: `[y](<(https://example.com/paren)>)` parses as an internal markdown link (LOW)

Real Hub occurrence in a 2021 Roundup: target `(https://github.com/…` counted broken and
fuzzy-matched to `Plugins/obsidian-tracker.md` at 0.57. A destination whose first non-`(`
character starts a scheme is external; at minimum never fuzzy-matched.

### BUG-22: The `summary` hint names a config key that does not exist (LOW)

MDN `summary` → `set \`--site-prefix\` (or \`[site] prefix\` in .hyalo.toml)`. The real key is
top-level `site_prefix`; following the hint writes a malformed config that silently falls back to
defaults. Iteration 267 was the hint-polish iteration.

### BUG-23: `[links.auto] exclude_titles = []` does not switch the built-in stop-list off (LOW)

DEC-286 and the runtime note say a configured list "replaces the built-in list entirely"; an
empty list still holds back the built-ins and prints the same note. Either honour the empty list
or say "a non-empty list".

### BUG-24: `mv --on-conflict <bogus>` is accepted silently and single-file `mv` ignores `--on-conflict skip` (LOW)

`mv n1.md n2.md --on-conflict bogus` → `target file already exists`; `--on-conflict skip` → the
same error instead of a skip; `--on-conflict bogus --dry-run` proceeds. The policy should be a
clap value enum and single mode should reject or honour it.

### BUG-25: Some user errors emit plain text and exit 2 in `--format json` mode (LOW)

`lint --files-from /nonexistent --format json`, `create-index --output /nonexistent/dir/idx`,
`find --glob '['`, `init --profile nope --format json` all exit 2 with a non-JSON line. The
top-level help promises exit 1 for user errors, 2 for internal errors. See UX-1 for the wider
exit-code taxonomy question.

### BUG-26: Batch `mv` collision message is wrong for a single source (LOW)

`mv --glob c1.md --to dest/ --apply` where `dest/c1.md` exists says `multiple sources map to the
same destination`. One source collides with an existing file.

### BUG-27: The `fields:` footer lists `score` but `--fields score` is rejected (LOW)

`find "broken links"` text footer prints `fields: …, score`; `--fields file,score` → `unknown
field "score"`. DEC-275 promises the footer round-trips. Drop `score` from the footer the way
`title_source` is (DEC-283), or accept it.

## UX issues

### UX-1: Exit-code taxonomy has three undocumented classes (LOW, judgement requested)

`find dataview plugin` (hyalo's own did-you-mean-quotes error) exits 2; `find --sort nope`
(hyalo's own validation) exits 1; every clap usage error (`--limit abc`, `--bogus`, `hyalo indx`,
missing required flag) exits 2. DEC-276 says exit 2 is reserved for internal errors, which is
already false for clap. Either make the two-positional case exit 1 or amend DEC-276 to "2 = clap
usage errors plus internal errors".

### UX-2: Hints on an indexed run drop `--index-file` (MEDIUM at 14k files)

MDN `find --index-file … --limit 1` emits `-> hyalo backlinks … --dir … --format text` without the
index flag; following it costs 1.2–1.4 s instead of 0.14 s. `--dir` and `--format` are threaded
through; the index flag should be too, also on `summary`'s `find --orphan` / `--broken-links` hints.

### UX-3: `links fix --dry-run` on full MDN takes 28.7 s and the "prefix stripped 0 of N" warning comes after the scoring pass (MEDIUM)

49770 of 49772 links are site-absolute; the fuzzy matcher scores each against 14375 files before
the warning that says to set `site_prefix` instead. Short-circuit scoring when the warning fires,
or print it first. Not a regression: GitHub Docs measures 4.20 s vs its 4.04 s baseline.

### UX-4: The mixed-type sort warning is evaluated after `--limit` (LOW)

`find --sort property:priority --limit 2` shows `critical, critical` (strings before numbers, the
exact artefact the warning exists for) with no warning; `--limit 0` and `--reverse --limit 2` warn.

### UX-5: The `SCHEMA` rule group cannot be selected (LOW)

Lint labels findings `SCHEMA`, but `--rule SCHEMA` → `no such rule` and `lint-rules list` has no
row. No way to run the schema pass alone.

### UX-6: Empty scaffold placeholders are reported twice per field (LOW)

`new` then `lint --detailed` on a type with three required typed fields: `required property
"rating" must not be empty` plus `property "rating" expected number, got null`, six errors for
three fields. DEC-285 wants the empty value to name exactly the fields to fill.

### UX-7: `types set/show/list --help` leak 12-space doc-comment indentation (LOW)

Paragraphs after the first are indented twelve spaces in `--help`; `-h` and every other command
are clean.

### UX-8: Wrong did-you-mean on `changelog add --type` (LOW)

`error: unexpected argument '--type'` followed by `did you mean '--property type=<value>'?`;
`changelog add` has no `--property`, the flags are `--category` and `--message`.

### UX-9: Missing-path handling differs between `--file`, `--files-from` and an empty list (LOW)

`--file good.md --file nope.md` loses the good row and prints both a warning and an error
envelope (exit 1); `--files-from` with the same list returns the good rows, `files_missing: 1`,
exit 0; `--files-from empty.txt --count` → `0` silently. `find --help` documents only the non-.md
and outside-vault skips.

### UX-10: `links fix --apply-fuzzy` text says "applied: 26" when 8 are at or above the floor (LOW)

Hub text: `Low-confidence matches (applied via --apply-fuzzy): 26`; JSON: 8 with `below_floor:
false`, 18 held. Say "26 candidates, 8 above 0.8".

### UX-11: The stop-list stderr note is one 1.6 KB line and misdescribes flag composition (LOW)

"showing the 5 noisiest of 30" then all 30 `--exclude-title` flags on one line; and `--exclude-title`
on the CLI composes with the built-ins while only the config key replaces them, which the note's
"make the choice explicit with --exclude-title" wording hides.

### UX-12: `hyalo config` reports `dir: "."` for a config every other command refuses (LOW)

A project-local `dir = "/abs/path"` or `../../…` escaping the config dir: `config --jq
'.results.dir'` → `"."`, while `links auto` exits with `is an absolute path, which a project-local
.hyalo.toml is not allowed to set`. `config` has `dir_out_of_bounds` for exactly this.

### UX-13: `lint --fix` JSON has no per-rule totals and truncation is silent to a `jq` consumer (LOW)

`rules_fired` is a bare count; a vault-wide "fixes by rule" needs `--limit 0 --max-per-rule 0`
plus a group-by over `files[].rule_groups[]`, and iteration 263's own AC jq does not match the
shape. A `rules_fixed: {rule: n}` map next to `rules_fired`, and a hint when `files_truncated`
is true, would make it scriptable without a new flag.

### UX-14: `enabled: null` in `lint-rules list` JSON for a rule at its default (LOW)

`true` is the honest effective value.

### UX-15: `--sort title` is byte-order, so lowercase and `{%`-prefixed titles sort after every capitalised title (LOW)

DEC-273 fixed direction, not collation; a case-insensitive title sort would match what the key
means to a human.

### UX-16: H1 fallback titles keep HTML comments (LOW)

`../docs/content/README.md` → `title: "Content <!-- omit in toc --><!-- markdownlint-disable -->"`,
`title_source: h1`.

### UX-17: `task toggle` JSON on CRLF files leaks `\r` into `text` (LOW)

`{"text":"task\r"}`; the bytes on disk are correct.

### UX-18: `hyalo deinit --dir /nonexistent` exits 0 after thirteen `skipped … (not found)` lines (LOW)

### UX-19: `types set 'bad name'` accepts a space and silently adds `validate_on_write = true` on first schema creation (LOW)

Nothing in the output mentions the `validate_on_write` side effect.

### UX-20: `okf index --dry-run` exits 1 when changes are pending (LOW)

Every other dry run (`okf log`, `links auto`, `links fix`, `set`, `lint --fix`) exits 0; this one
trips `set -e` scripts. Iteration 176's choice, but it is the odd one out.

### UX-21: Some malformed `--property` filters match nothing instead of being rejected (LOW)

`'a='`, `'a>='`, `'a=b=c'`, `'title[0]=1'`, `'title.b.c=1'` return 0 results with exit 0 while
`'=b'` and `--tag ''` are rejected. An empty comparison operand is not covered by any DEC.

### UX-22: `links fix` warns "site_prefix stripped 0 of 1 site-absolute links" for `[[/a]]` although the link resolved (LOW)

### UX-23: `title: 1e3` is reported as `"1000.0"` (LOW)

YAML 1.2 float rule; Obsidian shows `1e3`.

### UX-24: `set --tag` on `tags: [a, [b]]` reserialises the value to block style (`- - b`) (LOW)

Within the byte-preservation contract (only the touched key changes) but the one case where the
surrounding style flips.

### UX-25: `create-index --index --index-file` is rejected as "unexpected argument" (LOW)

`--index` is a subcommand flag and `--index-file` a global one; the hint text after the
outside-vault refusal suggests combining them. `--index-file` alone works.

Wishes that surfaced: `find --property type=iteration` normalising wikilink and list shapes the
way binding does (today it needs two filters); `tags rename` covering inline body `#tags` or the
help saying it does not; a `todo`-only task projection so the `completed-with-todos` view is
actionable; an `mv --on-conflict` value enum.

## Feature gaps (not bugs, no DEC found)

- **Markdown image syntax is invisible to the link extractor.** `![alt](file.png)` is skipped by
  design in `hyalo-core/src/links.rs` while `![[img.png]]` and `[alt](img.png)` are attachments.
  Whole-MDN histogram: `attachment 2, embed 1` against thousands of images, so a missing image can
  never surface in `find --broken-links`.
- **`<!-- markdownlint-disable … -->` is not honoured.** MDN's whitespace guide wraps a tab-laden
  `html-nolint` fence in `markdownlint-disable no-hard-tabs`; `lint --fix` replaced the tabs in a
  page whose point is showing tabs. Neither the rule-id nor the alias form nor the `-nolint`
  info-string suffix is recognised. MEDIUM by impact for a linter run on other people's corpora.
- **MDN's underscore anchors get no `suggested_fragment`.** 1242 of 1254 broken anchors on the
  CSS subtree are `#predefined_fallback_options` for `## Predefined fallback options` (Yari slug);
  a `_`↔`-` normalisation before DEC-268's prefix test would turn most into suggestions.
- **GitHub Docs links need redirects, not a prefix.** 1569 of 1624 "broken" files link to
  historical URLs served through `redirect_from:` frontmatter lists; `site_prefix = "actions"`
  resolves only 3 more files. An opt-in `[links] redirect_property` would make the graph usable.
- **Block references and slug anchors are never reported broken.** `[[Target#^nope]]` and
  `[[Target#section-one]]` pass; Obsidian would break the first.
- **Inline body tags are not in `hyalo tags`** and `--tag` never says so; DEC-282 scopes `tags
  rename` the same way.

## Verified working

Everything the ten iterations claimed was re-verified on the vault it targeted; the highlights
that go beyond the regression table:

- **Index parity (265/266)**: nine MDN commands byte-identical disk vs `--index-file`, BM25
  scores equal to the last digit; Hub `find --fields all`, `backlinks` on a 2190-backlink file,
  `properties`, `tags`, `--orphan`, `--dead-end`, `summary` identical after `del(.hints)`.
- **Link kinds (261/262)**: all six kinds found in the wild with correct `path`, `line`,
  `property`, `label`; `![[img.png|200]]` → attachment with label `200`; `[[Target#^blk]]`
  fragment kept; `[[Folder\note]]` and `%20` paths resolve; `[[日本語 ノート]]`, emoji filenames,
  `%% %%` comments and code spans behave. Case-only `mv a.md A.md` on macOS rewrites all eleven
  link forms including folded, literal and wrapped-quoted frontmatter values.
- **Iteration 269**: a folded `>` frontmatter value spanning two lines whose file has no other
  link to the target → `warning: … frontmatter wikilinks not rewritten`, JSON
  `frontmatter_links_skipped`, bytes untouched; MD034 `Retroma.md:65` → `<url>` then `<br`;
  frontmatter-only file → no MD047.
- **DEC-290**: `set --validate`, `append --validate`, dry-run included, and `validate_on_write`
  refuse with exit 1 and a `-q`-proof warning when `[schema]` fails to load; plain `set` writes
  with the warning; `types set` still edits the config in place.
- **`object-list` (DEC-287)**: all seven violation shapes reported with index, key and the
  `- ref: <value>` fix-it text; `types show` renders the three keys; dot-path `find` works;
  `autofixable: false` on pattern violations.
- **Find (264)**: every sort key ascending, nulls last both ways, `score` best-first; the five
  null/empty/absent forms give distinct counts on the real KB; typed ordering skips other kinds;
  all seven documented rejections exit 1 with the promised messages.
- **Scan (265)**: `[scan] exclude` consistent across `find`, `summary`, `tags`, `create-index`,
  `--index`, `backlinks`, `lint`, `mv`, with named excluded files refused by glob name; one
  stderr line for 28 unparsable kepano files, `-q` silent, `verbose_skips` restores excerpts;
  malformed config gates exactly `lint`, `find --strict`, `views run`.
- **Mutations (266/267)**: `properties rename` changes one line and preserves comments, quoting,
  folded scalars and position; `tags rename` renames the subtree and leaves `musical`; `mv`
  reports both counters in text and JSON; `new --dry-run` writes nothing, not even the parent dir.
- **Byte preservation**: `set`, `remove`, `append`, `mv`, `lint --fix`, `task toggle` on BOM,
  CRLF, no-EOL, 0-byte, `\n`-only, `---\n---\n`, multi-doc, fence-inside-body and 45 hostile
  scalar values touched only the addressed line (`diff | cat -v`), except BUG-2.
- **Robustness**: truncated, zeroed, byte-flipped and foreign-vault index files fall back to a
  disk scan with a one-line warning; 60 JSON error paths produce an envelope (exceptions in
  BUG-25); mode-444 files and symlinks handled; `--dir` on a path with spaces and non-ASCII from
  an unrelated CWD works.
- **Autofix on GitHub Docs**: 29 fixes in 14 files, all 18 hunks inspected, no Liquid tag, table
  or `{% data %}` heading damaged; the only corruption is BUG-3.
- **`links auto` stop-list (267)**: on the GitHub Docs actions subtree 734 of 851 mentions held
  back, 14 ambiguous titles withheld, and a sample of 10 remaining proposals all sensible.

## Performance

Medians of 3, seconds. No command exceeds 1.5× its baseline; the one deliberate cost is the
DEC-286 preview pass in `links auto`.

| Vault | Command | This run | Baseline |
|---|---|---|---|
| own KB | `find --limit 1` / `"broken links"` / `summary` / `--property status=completed` | 0.023 / 0.196 / 0.060 / 0.022 | 0.02 / 0.18 / 0.05 / 0.02 |
| Hub | `find --limit 1` / `find plugin` / `summary` | 0.17 / 0.96 / 0.44 | 0.14–0.18 / 0.89 / 0.42 |
| Hub | `find --broken-links --count` / `links fix --dry-run` / `lint --fix --dry-run` | 0.51 / 0.51 / 1.05 | — / 0.79 / 1.47 |
| Hub | `links auto --dry-run` | 0.88 | 0.48 |
| Hub | `mv` dry-run, 2190 backlinks / `create-index` (43 MB) | 0.36 / 0.58 | 0.25 (fewer backlinks) / 0.62 |
| MDN | `create-index` (124 MB) | 2.58 | 2.54 |
| MDN | `find --limit 1` disk / index | 0.61 / 0.14 | 0.56–0.88 / 0.11 |
| MDN | `find "flexbox gap" --limit 10` disk / index | 3.65 / 0.41 | 3.56–3.73 / 0.35 |
| MDN | `summary` disk / index | 1.39 / 0.44 | 1.16 / — |
| MDN | `find --property page-type=guide --limit 1` disk / index | 0.70 / 0.17 | 0.70 / 0.11 |
| MDN | `find --broken-links --count` disk / index, `lint --count` | 1.38 / 0.42, 3.35 | — |
| MDN | `links fix --dry-run` | 28.67 | — (UX-3) |
| GitHub Docs | `find --broken-links --count` / `--orphan --count` / `links fix --dry-run` | 0.36 / 0.34 / 4.20 | 0.29 / 0.28 / 4.04 |
| GitHub Docs | `create-index` (37 MB) / `lint --count` / `lint --fix --dry-run` / `links auto --dry-run` | 0.66 / 0.90 / 1.05 / 0.87 | — |

## What worked well

- The regression sweep is the cleanest since the Obsidian testbeds were introduced: nothing
  regressed, and the numbers the plans promised were met or beaten on the real vaults.
- Index parity is now exact, including BM25 scores, and the corrupt-index fallbacks never once
  returned wrong data.
- Byte preservation under mutation held across every hostile frontmatter shape the adversarial
  pass could construct, with one exception that hyalo itself can trigger (BUG-2).
- The `find` rejection messages, the `[scan] exclude` refusal naming the glob, the DEC-290 refusal
  text and the `object-list` violation messages are all specific enough to act on without reading
  the docs.

## Recommended next iterations

1. **Write safety** (BUG-1, BUG-2, BUG-13): content-hash or lock around read-modify-rename;
   column-0 closing fence plus an emitter guard; reject empty rename targets. Three fixes in
   `hyalo-core`, one PR.
2. **Autofix and link-rewrite corruption** (BUG-3, BUG-4, BUG-7): MD031 on unterminated fences;
   `site_prefix` case-mismatch rewrites keep the incoming form and the dry-run shows the written
   string; the ambiguity guard covers frontmatter links.
3. **Resolution completeness** (BUG-5, BUG-6, BUG-8, BUG-15, BUG-16, BUG-21): list-typed `type:`
   under a declared schema; `aliases:` as link targets with alias-aware fuzzy exclusion; anchor-only
   markdown links; `[[[` and `[[x]` capture boundaries; `(<(scheme:` destinations.
4. **Index and named-file honesty** (BUG-9, BUG-10, BUG-11, BUG-12, BUG-18): per-file
   `--broken-links` keeps anchor data; named unparsable and not-in-snapshot files are errors or
   upserts, never empty successes; newest per-file mtime in the stale probe; excluded count in the
   snapshot header.
5. **Hint and contract polish** (BUG-17, BUG-22, BUG-25, BUG-27, UX-1, UX-2, UX-3, UX-13): the
   `[site] prefix` hint, `--index-file` threading, exit-code taxonomy as a DEC, `rules_fixed`.
