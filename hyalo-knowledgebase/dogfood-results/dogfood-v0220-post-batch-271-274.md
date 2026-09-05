---
title: "Dogfood v0.22.0 — after iterations 271–274: zero regressions, alias semantics, list-indented fences, site_prefix cost"
type: research
date: 2026-09-05
status: active
tags: [dogfooding, obsidian, links, lint, mv, schema, index, performance]
related:
  - "[[dogfood-results/dogfood-v0220-post-batch-261-270]]"
  - "[[iterations/iteration-271-write-and-rewrite-safety]]"
  - "[[iterations/iteration-272-resolution-completeness]]"
  - "[[iterations/iteration-273-index-and-named-file-honesty]]"
  - "[[iterations/iteration-274-hints-help-and-contract-polish]]"
  - "[[iterations/iteration-275-alias-semantics-and-mv-guards]]"
  - "[[iterations/iteration-276-autofix-config-and-index-honesty]]"
  - "[[iterations/iteration-277-link-graph-parity-and-write-performance]]"
  - "[[decision-log]]"
---

# Dogfood v0.22.0 — after iterations 271–274

Binary `hyalo 0.22.0 (625c5c19510d 2026-09-05)`, `cargo install`ed from `main` after PR #322 and
verified against `git rev-parse HEAD` before any command ran. Four parallel explorers, one
testbed group each, every mutation on a scratch copy; both external checkouts verified clean by
`git status` at the end. Concurrent writes were not tested (DEC-292, non use-case).

| Testbed | Files | Role |
|---|---|---|
| `hyalo-knowledgebase/` (own KB) | 461 | regression of every item in the previous report, 274 polish, jq recipes, perf |
| `../obsidian-hub` | 6540 | aliases, `mv` ambiguity, embeds, anchors, `lint --fix`, index parity |
| `../kepano-obsidian` | 103 | property-rich Obsidian regression, `.base`, `mv` |
| `../mdn/files/en-us` | 14375 | `site_prefix` links fix, index parity, old-index behaviour, perf |
| `../docs/content` (GitHub Docs) | 3710 | nested YAML, Liquid-heavy autofix, `--sort title` |
| synthetic vaults (fence, emit, lint, links, contract, schema, rename, mv) | tiny | DEC-293/294/307 torture, byte preservation, exit codes |

Headline: **the 271–274 batch closed what it claimed.** Of the 51 items in the previous report
(BUG-2…29, UX-1…25), **44 are fixed, 6 closed by recorded decision, 1 partial (BUG-19), 0
regressed**. Every write in roughly 160 adversarial invocations touched only the addressed line,
with one cosmetic exception (BUG-33 below). Index parity is byte-identical on the Hub, kepano and
MDN. `links fix --dry-run` on full MDN went 28.7 s → 5.5 s with the right prefix and 2.7 s with
the diagnostic. Zero panics.

The new round found **47 bugs (6 HIGH, 16 MEDIUM, 25 LOW)** and 14 UX issues. Three of the HIGH
ones are in code the batch shipped: DEC-296 was written on a false premise about how Obsidian
treats aliases, the `mv` ambiguity guard for frontmatter links only works for files at the vault
root, and `markdownlint-disable-next-line` protects the wrong line. Two are older gaps the wider
testbeds exposed: list-indented fences are invisible to the 271 span pass, and a shipped skill
recipe is a bare vault-wide write that two explorers pasted verbatim. The dominant MEDIUM theme
is cost: per-file fsync makes every bulk write 25–50 s on the Hub, and `site_prefix` resolution
leaves the snapshot to stat the filesystem once per link.

## Regression of the previous report

| Range | Result |
|---|---|
| BUG-2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29 | FIXED, each re-run with the original repro (own KB, Hub, kepano, MDN, synthetic) |
| BUG-19 (`case_insensitive = "false"`) | PARTIAL: `config` reports it, but `[[categories/books]]` still yields `path: "categories/books.md"` for `Categories/Books.md` on macOS; `auto` gives the canonical path. The 274 outcome's "already canonical" claim is wrong (BUG-22 below) |
| UX-2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 25 | FIXED |
| UX-1, UX-12, UX-23, UX-24 | CLOSED by DEC-307 / iteration 274 decisions, behaviour as decided |
| Obsidian-vault report BUG-2/5/6/11 | STILL FIXED |

Selected numbers. Hub: `summary.links.broken` 154 (163 before 272), `find --broken-links --count`
83, HYALO006 111, 51 links `via: "alias"`, `links fix` fuzzy 18 (6 at or above the floor). MDN
with `--site-prefix en-US/docs`: kinds `attachment 1291, embed 15, external 21136, markdown
52577, wikilink 7` (all seven genuine `[[Prototype]]`-style tokens), broken links 522, broken
anchors 10929 with only 251 suggestions (BUG-7), `find --broken-links --count` 3993. GitHub
Docs: `lint --fix --dry-run` 1249 fixes, none inside `{% raw %}` or a matching disable region.

## Bugs found

Numbered globally; the explorer's own id is in brackets.

### HIGH

#### BUG-1: Obsidian does not resolve a bare `[[alias]]`, so DEC-296 marks links resolved that Obsidian renders as unresolved [obsidian BUG-1]

DEC-296 and the previous report both assumed Obsidian resolves `[[alias]]` when `alias` is in a
note's `aliases:`. It does not, by design: aliases feed the link suggester, which inserts
`[[Note|alias]]`; a hand-typed `[[alias]]` is an unresolved link and clicking it creates a new
note. Verified against the Obsidian help page on aliases ("Obsidian creates the link with the
alias as its custom display text, for example `[[Artificial Intelligence|AI]]`") and the forum
thread "Wikilink resolution does not honor frontmatter aliases (1.12.7)", where a moderator
states "This is not a bug … it's an intentional design decision"; the community plugin Alias
Linker exists to patch it in.

Effect on the Hub: 51 links carry `via: "alias"` and are excluded from `summary.links.broken`,
`--broken-links`, HYALO006 and `links fix`, although Obsidian shows them dead. `links fix` can
no longer propose the one rewrite that is right for `[[Leah]]`: `[[Leah Ferguson|Leah]]`.

Recommendation: mirror Obsidian. A bare alias link is broken; the alias map's job is to give
`links fix` an alias-backed `fixable` plan (`[[Leah]]` → `[[Leah Ferguson|Leah]]`, confidence
1.0, never fuzzy). Keep `[links] aliases` as the opt-in for vaults running Alias Linker, default
`false`, and amend DEC-296's "Why".

#### BUG-2: an ambiguous stem is tie-broken by an alias in `find`/`backlinks`/`summary` but not in `mv`; a rename silently flips the links to the other note [obsidian BUG-3]

Hub: `Plugins/avatar.md` (`aliases: [Avatar]`) and `Themes/Avatar.md` both exist. `find` resolves
`[[avatar]]` ×3 and `[[Avatar]]` to `Plugins/avatar.md, via: alias`; `backlinks` counts 5 vs 1.
`mv Plugins/avatar.md Plugins/avatar-plugin.md` lists all four under `skipped_ambiguous`, and
afterwards they resolve to `Themes/Avatar.md` with no broken report. Same for `[[blur]]`,
`[[christmas]]`, `[[terminal]]`, `[[zen]]`, `[[sekund]]`. DEC-296 says a filename match beats an
alias; an ambiguous filename match is still a filename match and must stay `path: null`.

#### BUG-3: the `mv` frontmatter ambiguity guard only sees links whose moved target sits at the vault root [obsidian BUG-2]

```text
mkdir -p Categories x References
printf -- '---\ntitle: Books\n---\n' > Categories/Books.md
printf -- '---\ntitle: twin\n---\n' > x/Books.md
printf -- '---\ncategories:\n  - "[[Books]]"\n---\nbody [[Books]]\n' > References/src.md
hyalo mv Categories/Books.md Categories/Library.md --dry-run
# skipped_ambiguous: [{line 5, property: null}]   <- the categories link is absent
hyalo mv Categories/Books.md Categories/Library.md --allow-ambiguous
# total_links_updated: 1 -> body becomes [[Library]], frontmatter stays [[Books]]
```

Matrix over moved/twin/source directories: only a root-level moved file reaches the guard. The
271 fixture (`a.md` at root) passes; every real layout fails. kepano repro: add `Notes/Books.md`,
`mv Categories/Books.md Categories/Library.md` → `total_links_updated: 0`, no
`skipped_ambiguous`, no stderr note, and `Out of Control.md` keeps `- "[[Books]]"`, which now
resolves uniquely to the twin. `--allow-ambiguous` promises to "rewrite them and say so" and does
neither.

#### BUG-4: `markdownlint-disable-next-line` suppresses the comment's own line, not the next one; `lint --fix` rewrites the protected line [adversarial BUG-1]

```markdown
<!-- markdownlint-disable-next-line MD019 -->
#   L8 id next-line (silent)

#   L15 trailing comment on same line <!-- markdownlint-disable-next-line MD019 -->
#   L16 line after trailing comment (silent)
```

`lint --fix` rewrote lines 8, 13 and 16 (the protected ones) and left line 15 (which should
fire) alone, for id and alias forms, standalone and trailing. `-disable-line` is correct.
DEC-294 lists `-disable-next-line` as honoured. Exit 0.

#### BUG-5: `lint --fix` MD034 wraps a URL inside a fenced code block that is indented inside a list item [scale BUG-1]

Real file `docs/content/code-security/how-tos/secure-your-supply-chain/manage-your-dependency-security/removing-dependabot-access-to-public-registries.md:224`:

```text
222:    1. Add the registry to a `.yarnrc.yml` file …
223:     ```
224:     npmRegistryServer: "https://private_registry_url"
225:     ```
```

`lint --fix` wrote `npmRegistryServer: "<https://private_registry_url>"`, corrupting the YAML
sample. The fence is a 4-space-indented ```` ``` ```` under a numbered item, valid CommonMark.
The 271 `BodySpans` pass does not recognise list-indented fences, so every rule it exempts
(MD009/011/012/019/022/023/034/042) can fire inside such a block. Same class as the previous
BUG-3/BUG-28.

#### BUG-6: a shipped skill recipe is a bare vault-wide write; two explorers pasted it and rewrote 461 and 10530 files [own UX-A, scale UX-1]

`crates/hyalo-cli/templates/skill-hyalo.md:235` shows
`hyalo set --glob '**/*.md' --property status=draft --jq '.results.skipped_count'` as the example
for `skipped_count`, with no `--dry-run`. The own-KB explorer ran it as written (461 files, one
line each, restored with `git checkout`); the scale explorer's recipe runner appended `--dir` and
ran it against the real MDN checkout for 120 s (10530 files, restored with `git restore`). On MDN
it also collapsed 2014 `status:` lists to a scalar (DEC-270, reported). The `check-jq-recipes`
gate's header says "documentation must never invite a reader to paste a write", then silently
appends `--dry-run` instead of failing. Rated HIGH because it happened twice in one session to
agents following the shipped instructions. Fix: `--dry-run` in the shipped text and a gate that
fails on any mutating recipe lacking it, as it already does on `--apply`.

### MEDIUM

#### BUG-7: no `suggested_fragment` when the folded fragment equals the whole heading, and `_` is not folded on resolution [obsidian BUG-4, own NEW-1, scale BUG-5]

```text
## Predefined fallback options
[[#predefined_fallback_option]]   -> suggested_fragment: "Predefined fallback options"
[[#predefined_fallback_options]]  -> suggested_fragment: null      <- the MDN case
```

`crates/hyalo-core/src/anchor.rs:199` skips headings with `heading.len() <= needle.len()`, so
an equal-length fold match is excluded, and resolution folds `-` and `%20` but not `_`.
DEC-298's own motivating example (`#Browser_compatibility`) is this shape. On MDN: 10929 broken
anchors, 251 suggestions; css copy 1255 / 34.

#### BUG-8: `mv` leaves the moved file's own body self-links pointing at a different note when the stem is ambiguous, and reports nothing [adversarial BUG-2]

`kb/a.md` (body `self [[a]] and [[a#Top]]`, `related: "[[a]]"`) with `kb/sub/a.md` present.
`mv kb/a.md kb/z.md` → frontmatter becomes `[[z]]`, body still `[[a]]` and `[[a#Top]]`, which
now resolve to `sub/a.md`; no `skipped_ambiguous`, exit 0.

#### BUG-9: a `[[[x]], [[y]]]` frontmatter flow list is a link for `find`/`backlinks` but `mv` neither rewrites nor reports it [obsidian BUG-5]

`related: [[[iterations/iteration-206]], [[Target]]]` resolves and appears in `backlinks`;
`mv iterations/iteration-206.md iterations/done.md` omits the file from `updated_files` and
`frontmatter_links_skipped` is null.

#### BUG-10: `mv` refuses an absolute in-vault destination and cannot target the vault root [obsidian BUG-7, adversarial L-13, own NEW-2]

`mv sub/a.md $R/kb/a3.md` → `target path must be relative and within the vault` although it is,
while an absolute source is accepted; `mv --help` says the destination is resolved like the
source. `mv kb/sub/a.md --to kb/` → `destination directory does not exist … create kb/ first`;
`--to kb` → `must end with .md`; from inside the vault `--to ./` → `/a.md` refused. DEC-304's
strip leaves an empty string for the root.

#### BUG-11: `--index-file /nope` is a silent full disk scan with exit 0, and `-q` hides the only warning [adversarial BUG-5]

`--files-from /nope`, `--dir /nonexistent` and `create-index --output` all exit 1; a named
index that cannot be read is ignored, the expensive fallback runs, and `-q` silences the warning.

#### BUG-12: a snapshot built by an older binary is served without any version warning [scale BUG-3]

`../mdn/files/en-us/.hyalo-index` (Sep 3, pre-272) with the new binary: `summary --index`
answers `links {total: 49774, broken: 49772}` vs disk `{51075, 49784}`, orphans 4279 vs 4219,
no `attachment` kind, no warning. Only the mtime probe ever fires; there is no format check.

#### BUG-13: `site_prefix` link resolution stats the filesystem once per link, even from the index [scale BUG-6]

| command | no prefix | `--site-prefix en-US/docs` |
|---|---|---|
| `summary` index | 0.55 s | **4.65 s** (sys 3.8) |
| `find --broken-links --count` index | 0.44 s | **2.37 s** |
| `create-index` | 3.22 s | **6.44 s** |

Time is system time, consistent with `classify_link` probing `Path::is_file()`; ~50k stats per
query on MDN. The snapshot already holds the file set. Aliases are not the cause (`aliases =
false` saves 0.5 s of 5.5 s).

#### BUG-14: every write costs 8–11 ms of fsync; `lint --fix` on 6430 Hub files takes 49 s, `mv` with 2190 inbound links 25 s, bulk `set` on MDN runs at 88 files/s [obsidian UX-1, scale UX-1]

Dry-run 1.3 s vs applied 49 s. The 271 atomic-write path serialises temp-file + fsync + rename
per file, with no progress output.

#### BUG-15: `--site-prefix` given on the CLI is dropped from every hint; the chain silently changes the answer [scale BUG-2]

`find --broken-links --site-prefix en-US/docs` hints `hyalo find --broken-links --index --dir …`
without the prefix; followed as printed it runs with the derived prefix `en-us`, matches the old
in-vault index without warning, and answers **10153** files instead of **3993**. `--dir`,
`--format`, `--index-file` are threaded (UX-2 fixed); the prefix is not. With `site_prefix` in
`.hyalo.toml` every hint is right.

#### BUG-16: `summary` counts attachment links as graph edges; `find --orphan/--dead-end` do not [scale BUG-7]

MDN: `summary.orphans 3403` vs `find --orphan --count 3428`; dead-ends 816 vs 851. The 25 extra
are files whose only outbound links are `kind: attachment`, which `find --help` says are not
edges.

#### BUG-17: `fuzzy_fixes[]` carry no `emitted_target` [scale BUG-4]

`links fix --help` promises it on every plan; case-mismatch and applied plans have it, the fuzzy
bucket (the one `--apply-fuzzy` writes from guesses) does not. MDN 1/1, docs-actions 645/645.

#### BUG-18: fuzzy confidence ignores the runner-up; 4 of 6 above-floor Hub proposals are wrong [obsidian UX-3, scale UX-5]

`Cat → CatMuse.md 0.87` with five `cat*` People notes, `jamesb → jamesgreenblue.md 0.885` with
eight `james*`, `paulbricman → paultreanor.md 0.854`, `obsidian-floating-toc-plugin →
obsidian-plugin-toc.md 0.857`. On directory-index corpora the parent `index.md` is a 0.9
neighbour of any missing child (`…/tabindex → global_attributes/index.md 0.9125`). When the
runner-up is within a small margin, drop below the floor, the way `ambiguous` protects stems.

#### BUG-19: `lint --max-per-rule 0` shows zero violations; help says `0 = unlimited` [adversarial BUG-3]

`{"count":5,"rule":"MD019","shown":0,"truncated":true,"violations":[]}`; text prints only
`… (5 more MD019)`. `--limit 0` correctly means unlimited.

#### BUG-20: `[schema]` and `[schema.types.<t>]` accept unknown keys silently; a typo yields a schema that validates nothing [adversarial BUG-4]

`requried = ["title", "status"]` → `config` not malformed, `types show note` has no Required
line, `lint --strict` exits 0. `[scan] bogus = 1` is rejected; property-level unknown keys are
rejected; the type and schema levels are not.

#### BUG-21: from inside a vault subdirectory a bare path resolves to the vault-root file even when the CWD has one of that name, silently [adversarial BUG-6]

From `kb/sub/` with both `kb/a.md` and `kb/sub/a.md`: `set a.md`, `mv a.md x.md`, `find --file
a.md` all act on the root file, exit 0. Documented, but the trap is silent. No new flag: warn
when `<cwd>/<path>` exists and differs from `<vault>/<path>`; and make `mv --to ../deep/` say
"path contains `..`" like the source check does.

#### BUG-22: `[links] case_insensitive = "false"` returns a non-canonical path on macOS [own BUG-19 carry-over]

`[[categories/books]]` → `path: "categories/books.md"` for `Categories/Books.md`; `auto` gives
the canonical path. The 274 outcome said the path was already canonical.

### LOW

- **BUG-23** wikilink targets are not whitespace-trimmed: `[[ Leah Ferguson ]]` unresolved, HYALO006 fires; Obsidian trims [obsidian BUG-6, adversarial L-4].
- **BUG-24** `summary --index` reports `files.skipped: 0` and drops the skipped directory row (Hub 1, kepano 28); the header records `excluded` but not `skipped` [obsidian BUG-8].
- **BUG-25** batch `mv` dry-run aborts on a destination collision instead of listing it; batch JSON has `total_files_updated: null` [obsidian BUG-9].
- **BUG-26** an alias collision under `links fix` has `candidates: null`, and HYALO006 says "does not resolve" instead of "ambiguous" [obsidian BUG-10].
- **BUG-27** `lint-rules show SCHEMA` hints `=> hyalo lint-rules set SCHEMA --enabled false`, which fails with `no such rule` [own NEW-3].
- **BUG-28** `--dir .` with an empty `.hyalo.toml` says "`.hyalo.toml` already sets `dir = "."`" (`run.rs:1320`) [own NEW-4].
- **BUG-29** `hyalo --help` and `lint --help` still say exit 2 is internal only; DEC-307 assigns clap usage errors to 2 [own NEW-5].
- **BUG-30** DEC-302's blind spot is about 2 s (whole-second mtimes + 1 s tolerance), not "the same whole second" [own NEW-6].
- **BUG-31** split frontmatter link `target` carries trailing whitespace (`"t1 "`) in `frontmatter_links_skipped` [own NEW-7].
- **BUG-32** `find --help` promises `(via alias)` in text mode; nothing is printed [own NEW-8, obsidian UX-2].
- **BUG-33** `set`/`append` rewrite a closing fence that has trailing whitespace (`--- ` → `---`), the only non-addressed byte touched in the adversarial pass [adversarial L-1].
- **BUG-34** an opener with trailing whitespace (`--- `) is not frontmatter; `set` prepends a second block above it, contradicting DEC-293's wording [adversarial L-2].
- **BUG-35** `set`/`append`/`remove` on an unparsable file print a plain `error:` line before the JSON envelope, whose hint says "see the error above" [adversarial L-3].
- **BUG-36** `[[a#Heading One#Sub Two]]` nested heading path is `broken_anchor: true` with no suggestion; decide-or-implement next to DEC-299 [adversarial L-5].
- **BUG-37** `[[./a]]` reports `path: "./a.md"` instead of the canonical path [adversarial L-6].
- **BUG-38** `tags rename` reserialises a flow-style `tags: [..]` to block style; `set --tag` keeps flow [adversarial L-7].
- **BUG-39** batch `mv` prints every ambiguous-link warning twice [adversarial L-8].
- **BUG-40** `1. [ ]` and `-  [ ]` (two spaces) are not tasks; `- [ ]no space` is [adversarial L-9].
- **BUG-41** `set sci=1e3` writes `1000.0`; `null`/`~` become strings; keys `y`/`n`/`yes`/`no` are quoted — undocumented coercion [adversarial L-10].
- **BUG-42** `required = ["title"]` implies `string`, so `title: 2024` fails `SCHEMA` and `set --validate` [adversarial L-11].
- **BUG-43** an unknown rule id or alias in a suppression comment is accepted silently; markdownlint reports it [adversarial L-12].
- **BUG-44** `summary` prints a near-duplicate-value warning on a 12-file vault for values sharing no letters [adversarial L-14].
- **BUG-45** `links fix` JSON `broken_anchors` is always 0 (MDN 10929 in `find`); undocumented key [scale BUG-8].
- **BUG-46** the two UX-3 warnings disagree on their count (49767 vs 49776) [scale BUG-9].
- **BUG-47** `find --broken-links` still hints `links fix` when every broken link is site-absolute, which iteration 274 listed as shipped [scale BUG-10].

## UX issues

- **UX-1** (MEDIUM) `set` reports a same-value write as `skipped` with no reason; indistinguishable from a parse-skip or schema-skip in JSON [adversarial].
- **UX-2** (LOW) `mv` text mode drops the `property` tag the JSON `skipped_ambiguous` entries carry [adversarial].
- **UX-3** (LOW) `--on-conflict` has no `overwrite`; fine as a decision, say why in `mv --help` [adversarial].
- **UX-4** (LOW) the shipped `hints` recipe (`skill-hyalo.md:251`) can only return `[]` because `--jq` strips hints [own UX-B].
- **UX-5** (LOW) the `.claude/CLAUDE.md` broken-links recipe hides the fragment; on the own KB it prints `decision-log.md:53 decision-log` 23 times [own UX-C].
- **UX-6** (LOW) `find --broken-links --format text` prints all ~100 links of `decision-log.md` to show 5 broken anchors [own UX-D].
- **UX-7** (LOW) MD010 rewrites tabs inside fenced code with no per-rule opt-out; 215 fixes inside Go samples on GitHub Docs where gofmt mandates tabs [obsidian UX-4, scale G3].
- **UX-8** (LOW) the stale-index warning names the witness on one run and not the next (directory vs per-file probe) [obsidian UX-5].
- **UX-9** (LOW) `hyalo config` derives `site_prefix` from the directory name (`obsidian-hub`, `en-us`); on MDN the derived `en-us` strips nothing and silently matches the old index (see BUG-15) [obsidian UX-6].
- **UX-10** (LOW) `<https://…>` and `<obsidian://…>` autolinks are not inventoried at all [obsidian UX-7].
- **UX-11** (LOW) `hyalo find dup.md` (bare existing filename) is a text search returning "No results" plus a hint that knows it is a file [scale UX-2].
- **UX-12** (LOW) `--property 'versions.fpt!=*'` → 0 with 1249 files lacking the key; `!=` on absent keys is undocumented [scale UX-3].
- **UX-13** (LOW) `find --index --file <missing>` prints the stale-index warning before `file not found` [scale UX-4].
- **UX-14** (LOW) HYALO005 could name the opener-with-trailing-whitespace shape instead of the body-side MD009 [adversarial].

## Feature gaps (no DEC found)

- **G1** MDN slug encoding: 267 of 450 unresolved `/en-US/docs/…` links contain `:` or `*`, stored as `_colon_`/`_star_`/`_doublecolon_` directories; a `[links] slug_map` or a documented MDN profile would resolve them [scale].
- **G2** basename fallback on directory-index corpora: `X` never matches `**/x/index.md`, so MDN-style relocations get no proposal [scale].
- **G3** `ambiguous: true` with candidates on `--fields links` records, so ambiguous stems and alias collisions can be told from missing notes without `links fix` [obsidian].
- **G4** a snapshot version stamp in `summary --index` / `hyalo config` (follows from BUG-12) [scale].
- **G5** a way to write a YAML null with `set`, and a documented coercion table (BUG-41) [adversarial].
- **G6** `--fields links` filtered by kind, to replace the 20 MB dump + jq needed for a kind histogram; likely a `--jq` recipe rather than a flag [scale].

## Verified working

- **DEC-293** across every reader and writer: `find --file`, `read --frontmatter`, `set`, `append`, `remove`, `properties rename`, `lint` agree; the emitter double-quotes exactly the scalars that would re-trigger the old bug; CRLF, BOM, no trailing newline, YAML comments and a 42-key hostile file all survive with single-line diffs.
- **DEC-294** scoping on GitHub Docs is precise: only the named rules are muted, `{% raw %}` untouched, MD031 silent at unterminated openers; `disable`, `enable`, `disable-line`, `disable-file`, aliases and comma lists all hold (only `-next-line` is wrong, BUG-4).
- **DEC-295 / CASE-2** end to end: 0 case plans on 49767 prefixed MDN links; four form-preserving rewrites on the css copy whose applied bytes equal the dry-run's `emitted_target` character for character.
- **DEC-296 rules as written** (unique alias, filename beats alias, shared alias ambiguous, case-folded, scalar form, `[[alias#h]]`, `[[alias|label]]`, frontmatter with `property`, embed and markdown forms, `[links] aliases = false` visible in `config`, index parity, `mv` keeps alias links valid). The premise is wrong (BUG-1); the mechanics are not.
- **DEC-297** on Obsidian data: 2897 `obsidian://` external, 55 `.base` and 150 image attachments resolved by basename, `cover: "[[x.jpg]]"` an attachment with its property, `<(https://…)>` external; on MDN 1291 image attachments, no macro false positives.
- **DEC-301–306**: unparsable `--file` exits 1 with line/column; `--files-from` counts; `--index --file` upserts a four-deep unseen file; the stale probe names the witness for a same-size overwrite (+0.06 s on MDN); `summary --index` carries `excluded`; `mv` destination forms land in-vault from three CWDs; `--on-conflict` is a value enum honoured in both modes.
- **DEC-307**: 25 provoked user errors all exit 1 with a JSON envelope on stderr under `--format json`; clap cases exit 2; one leak (BUG-35).
- **Capture boundaries**: `[[a] b [[c]]`, `[[[a]]`, `[[a]]]`, `[[]]`, `[[` at EOF, inline and fenced code, `[y](<(https://…)>)`, `[y](<a b.md>)`, `![[img.png|200]]`, `[[/a]]` all behave.
- **Index parity** byte-identical on Hub, kepano and MDN for `--broken-links`, `--fields links`, `--orphan`, `--dead-end`, `summary`, BM25 top-10 with scores, `--sort title`.
- **jq recipes**: 25/25 unique shipped recipes execute on the own KB and MDN; no `IN()` errors.
- **`lint --fix` on the Hub** (6430 files, 148,933 diff lines): nothing but trailing whitespace, blank lines, final newlines, `<url>` wraps and tab expansion; no frontmatter key, `{{…}}`, disable region or block scalar touched.

## Performance

Own KB, hyperfine `-N -w 2 -r 7`; Hub best of 3; MDN medians of 2–3, `/usr/bin/time`.

| Vault | Command | Now | Previous | Ratio |
|---|---|---|---|---|
| own KB | `find --limit 1` / `find "broken links"` / `summary` / `--property status=completed` | 0.026 / 0.224 / 0.089 / 0.026 s | 0.023 / 0.196 / 0.060 / 0.022 | 1.1× / 1.1× / 1.5× / 1.2× |
| Hub | `find --limit 1` disk / index | 0.15 / 0.07 s | — | — |
| Hub | BM25 `dataview` disk / index | 0.94 / 0.13 s | — | — |
| Hub | `summary` disk / index | 0.56 / 0.28 s | — | — |
| Hub | `find --broken-links --count` disk / index | 0.65 / 0.37 s | 0.59 (272) | 1.1× |
| Hub | `links fix --dry-run` | 0.62 s | — | — |
| Hub | `lint --fix --dry-run` / applied (6430 files) | 1.26 / **49.05 s** | — | BUG-14 |
| Hub | `mv` with 2190 inbound links | **25.25 s** | — | BUG-14 |
| MDN | `create-index` no prefix / prefix | 3.22 / **6.44 s** | 2.58 | 1.25× / **2.5×** |
| MDN | `find --limit 1` disk / index | 0.68 / 0.20 s | 0.61 / 0.14 | 1.3× / 1.4× (stale probe) |
| MDN | BM25 `flexbox gap` disk / index | 4.01 / 0.47 s | 3.65 / 0.41 | 1.1× |
| MDN | `summary` disk / index, no prefix | 2.1 / 0.55 s | 1.39 / 0.44 | 1.5× / 1.25× |
| MDN | `summary` disk / index, prefix | 5.1 / **4.65 s** | — | BUG-13 |
| MDN | `find --broken-links --count` disk / index, no prefix | 1.71 / 0.44 s | 1.38 / 0.42 | 1.24× |
| MDN | `find --broken-links --count` disk / index, prefix | 3.6 / **2.37 s** | — | BUG-13 |
| MDN | `lint --count` | 4.56 s | 3.35 | 1.36× |
| MDN | `links fix --dry-run` no prefix / prefix | 2.67 / 5.49 s | 28.67 | **0.09× / 0.19×** |
| MDN | `set --glob '**/*.md'` (one key) | >120 s for 10530 files | — | ~88 files/s, BUG-14 |
| Docs | `find --limit 1` / BM25 / `summary` disk / index | 0.15 / 1.11 / 0.60 s ; 0.10 / 0.16 / 0.52 s | — | — |
| Docs | `find --broken-links --count` / `--orphan --count` | 0.47 / 0.45 s | 0.36 / 0.34 | 1.3× |
| Docs | `create-index` / `lint --count` / `lint --fix --dry-run` / `links fix --dry-run` | 0.80 / 0.96 / 1.11 / 4.64 s | 0.66 / 0.90 / 1.05 / 4.20 | ≤1.2× |

Flagged > 2×: MDN `create-index` with a prefix and every indexed link-graph query on a
`site_prefix` vault (BUG-13), plus the absolute cost of bulk writes (BUG-14). Everything else is
within 1.5×; the ~1.3× on link queries is the alias pre-pass plus image links.

## What worked well

- Zero regressions across 51 re-run items, and the deviations recorded in the four Outcome sections match what the binary does (except the BUG-22 canonical-path claim).
- Byte preservation held even through the two recipe incidents: 461 + 10530 files, one line each, trivially reviewable and reversible with git.
- The new error texts are specific and actionable: glob parse cause, `a=b=c` did-you-mean, `--to nodir/` hint, `create-index --index` explanation, the zero-result hint naming existing values with counts, `--allow-outside-vault`, the index-mismatch refusal.
- `links fix` on a `site_prefix` vault is now honest and fast: diagnostic first, scoring skipped, 28.7 s → 2.7 s.
- The `--index` path is honest: unseen named files read from disk with a note, unknown paths refuse, in-place edits older than 2 s warn with the file name.
- `object-list` schema errors and enum violations carry did-you-mean; `hyalo new` refuses `--property` with a copy-pasteable follow-up.

## Recommended next iterations

Three plans, folded to keep loop overhead down; each part names the bugs it closes.

1. **[[iterations/iteration-275-alias-semantics-and-mv-guards]]** — DEC-296 amended to Obsidian semantics with an alias-backed `links fix` plan (BUG-1, 2, 26, 32); the `mv` ambiguity guard for every layout, body self-links, flow lists, absolute and root destinations, batch reporting (BUG-3, 8, 9, 10, 25, 31, 39, UX-2); anchor suggestion equality and `_` folding, trimming, `./`, nested heading DEC (BUG-7, 23, 36, 37).
2. **[[iterations/iteration-276-autofix-config-and-index-honesty]]** — `disable-next-line`, list-indented fences, unknown suppression ids, `--max-per-rule 0`, MD010 opt-out decision (BUG-4, 5, 43, 19, UX-7); recipe safety in the shipped text and the gate (BUG-6, UX-4, 5); schema unknown keys, `--index-file` missing, snapshot format version, CWD-path warning, `case_insensitive = "false"` (BUG-20, 11, 12, 21, 22, G4); the write-side LOWs and help lines (BUG-27, 28, 29, 30, 33, 34, 35, 38, 40, 41, 42, 44, UX-1, 3, 14).
3. **[[iterations/iteration-277-link-graph-parity-and-write-performance]]** — batched/parallel atomic writes with progress (BUG-14); `site_prefix` resolution from the snapshot's file set (BUG-13); `summary` vs `find` edge parity and `files.skipped` (BUG-16, 24); `links fix` reporting: fuzzy `emitted_target`, runner-up margin, `broken_anchors`, warning counts (BUG-17, 18, 45, 46); hint threading of `--site-prefix` and the site-absolute hint, derived-prefix note (BUG-15, 47, UX-9, 13); the remaining read-side UX (UX-6, 8, 10, 11, 12) and DEC-or-implement on G1, G2, G3, G6.
