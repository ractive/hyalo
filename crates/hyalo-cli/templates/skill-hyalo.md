---
name: hyalo
user_invocable: false
description: >
  Use the hyalo CLI instead of Read/Edit/Grep/Glob when working with markdown (.md) files
  that have YAML frontmatter. This skill MUST be consulted whenever Claude is working with
  markdown documentation directories, knowledgebases, wikis, notes, Obsidian-compatible
  collections, Zettelkasten systems, iteration plans, or any collection of .md files with
  frontmatter. Trigger this skill when: searching or filtering markdown files by content,
  tags, or properties; reading or modifying YAML frontmatter; managing tags or metadata
  across documents; toggling task checkboxes in markdown; getting an overview of a
  documentation directory; querying document properties or status fields; bulk-updating
  metadata across many markdown files; or when you find yourself repeatedly using
  Grep/Glob/Read on .md files. Even if the user does not mention "hyalo" by name, use this
  skill whenever the task involves structured markdown documents with frontmatter.
---

# Hyalo CLI — Preferred Tool for Markdown with Frontmatter

Hyalo is a fast CLI for querying and mutating YAML frontmatter, tags, tasks, and structure
in directories of markdown files. Its killer features are combined filtering (e.g.
`hyalo find -e "regex" --property status!=done --tag feature`) which you can't easily
replicate with Grep/Glob, and bulk mutations (`hyalo set --where-property`) that replace
multiple Read + Edit calls.

Filters combine freely — content search + property conditions + tag + section + task status
in a single call, something impossible with Grep/Glob alone:

```bash
hyalo find "error handling" --property status!=completed --tag iteration --section "Tasks" --task todo
```

## BM25 Full-Text Search

The positional argument to `find` triggers BM25 ranked full-text search with automatic
stemming ("running" matches "run", "runner", etc.). Results are sorted by relevance score
by default (unless `--sort` is specified).

```bash
hyalo find "rust"                        # single term, stemmed
hyalo find "rust programming"            # AND: both terms required (implicit)
hyalo find "rust OR golang"              # OR: either term matches
hyalo find "rust -java"                  # NOT: exclude documents with "java"
hyalo find '"error handling"'            # Phrase: exact consecutive match (after stemming)
hyalo find '"error handling" -panic'     # Phrase + negation combined
hyalo find "rust OR golang -obsolete"    # Mixed: either rust or golang, not obsolete
```

For literal pattern matching (not stemmed), use regex: `hyalo find -e "exact_string"`.

Stemmer language: `--stemmer french` (or the older `--language french`) selects the French
Snowball stemmer for BM25 tokenization. Accepts full names (english, german, french, …) or
ISO 639-1 codes (en, de, fr, …). This is *not* markdown code-block language filtering.
Per-file override via frontmatter `language: french`. Config default via
`[search] language = "french"` in `.hyalo.toml`.

Property filters support: `K=V` (eq), `K!=V` (neq), `K>=V`/`K<=V`/`K>V`/`K<V` (comparison),
`K` (existence), `!K` (absence — files missing the property), `K~=pattern` or `K~=/pattern/flags`
(regex match on value; for list properties, matches if any element matches), and the value
shapes `K=null` / `K!=null` / `K=[]` / `K!=[]`:

```bash
hyalo find --property '!status'           # files missing the status property
hyalo find --property 'title~=draft'      # title contains "draft"
hyalo find --property 'title~=/^Draft/i'  # case-insensitive regex on title
hyalo find --property 'aliases=null'      # present, but the value is a YAML null
hyalo find --property 'aliases!=null'     # present AND non-null
hyalo find --property 'aliases=[]'        # present, and an empty list
```

**Null vs empty vs absent (iter-264, DEC-274):** `!K` is *absent*, `K=null` is *present with a
YAML null* (`~`, `null`, or an empty value), `K=[]` is *present and an empty list*. A list
containing a null (`aliases: [null]`) matches none of them — the value's own type is what is
tested, so `K=null` and `--fields properties-typed` (`type: "null"`) always agree.

**Comparisons are typed (DEC-274):** `>`, `>=`, `<`, `<=` compare numerically when both sides
parse as numbers (`rating>=6` matches `rating: "7"`), by date when both parse as ISO dates, and
as text only when both are plain strings. A value of any other kind never matches, so
`last>=2023-09-01` skips `last: "[[2022-04]]"` instead of comparing it as text.

**Rejected input (DEC-276, BUG-23/24):** the regex operator is `~=`, never `=~` — `title=~/pat/`
is now a hard error naming `~=` (it used to be silently read as equality against the literal
value `~/pat/`, which matched every YAML null in the vault). An empty pattern (`title~=` or
`title~=//`) and an empty selection (`--fields ''`, `--fields ,`) are errors too; all of them
exit 1, like every other bad argument.

`K` may be a **dot-path** into nested frontmatter. A literal dotted key in a flat map is
tried first; otherwise the path is walked. Maps descend by key, and sequences descend too:
a numeric segment pins one element, any other segment auto-descends into *every* element and
collects the hits — so the usual list semantics apply (`=`/`~=` match when any element
matches, `!=` when none does):

```bash
hyalo find --property contact.email=team@example.com   # contact: {email: ...}
hyalo find --property contacts.email=ada@example.com   # contacts: [{name, email}, ...] — any element
hyalo find --property contacts.0.email=ada@example.com # first element only
hyalo find --property '!contacts.phone'                # no element has a phone
```

**The promoted title (DEC-283).** `title` is resolved in three steps: a scalar frontmatter
`title`, else the first H1 heading, else **the filename stem** — Obsidian's own behaviour, so
a vault whose notes carry no `title` property still sorts and filters usefully instead of
showing `title: (none)` everywhere. JSON reports which one it was under `title_source`
(`"property"`, `"h1"` or `"filename"`), so a consumer can tell an authored title from a
derived one. `--title` and `--property title~=` both match the promoted value — neither is
frontmatter-only. To test the raw frontmatter key, use `--property title` / `--property
'!title'`.

Case-insensitive substring by default; use `"/regex/"` for regex.

```bash
hyalo find --title "meeting"           # substring match on the promoted title
hyalo find --title "/^Design/i"        # regex on the promoted title
hyalo find --limit 5 --format json --jq '[.results[] | {file, title, title_source}]'
```

Neither one sees body prose. An identifier that lives in a `##` heading — `DEC-251` in a
decision log, say — is not a title, so `--property 'title~=/DEC-25/'` correctly returns
nothing. When that happens hyalo checks whether the same regex occurs in body text and, if
it does, leads the zero-result hints with the body search that works
(`hyalo find -e 'DEC-25'`). Read the hints before re-guessing the filter.

`--section` uses case-insensitive **substring** matching by default — `"Tasks"` matches
`"Tasks [4/4]"`, `"My Tasks"`, etc. Use `"/regex/"` for regex. Prefix `##` to pin heading level.

`--glob` supports negation with `!` prefix to exclude files: `--glob '!**/draft-*'`.

`--glob` is also how you address sequence-keyed documents (iterations, decisions, ...):
the number may be zero-padded and the file archived in a subdirectory, so prefer the
recursive form — `find --glob '**/iteration-02-*.md'` reaches both `iterations/iteration-2-*.md`
and `iterations/done/iteration-02-links.md`. (`--file` is exact, `--glob` is the only
globbing flag; single-file commands like `read`/`set` have no `--glob` — resolve with
`find --glob ... --filenames-only`, then pass the exact path.)

`--sort` controls result ordering. Available: `file` (default), `modified`, `date`, `title`,
`backlinks_count`, `links_count`, `score`, or `property:<KEY>` for any frontmatter property.

**Direction (iter-264, DEC-273):** every key sorts **ascending** and `--reverse` inverts it, so
`--sort backlinks_count --reverse` is "most linked first" exactly as `--sort modified --reverse`
is "newest first". `score` is the one exception — it ranks best-match-first (descending
relevance), and `--reverse score` puts the weakest match first. Files whose sort property is
missing or null always sort **last**, in both directions. (Before 0.23, `backlinks_count` and
`links_count` sorted descending, so `--reverse` on those two keys meant the opposite of what it
means everywhere else — a script relying on the old order needs its `--reverse` flipped.)

```bash
hyalo find --sort modified --reverse --limit 10   # recently modified files
hyalo find --sort property:priority                # sort by custom property (nulls last)
hyalo find --sort backlinks_count --reverse        # most-linked files first
```

Without `--fields`, every result item carries `file`, `modified`, `size` (bytes), `lines`,
`title`, `properties` and `tags` — read `size`/`lines` before a `read` to know what a file
will cost.

`--fields` is an **exact projection**: the result carries exactly the fields you name, plus
`file`, which names the result and is never dropped. So `--fields title` returns `{file, title}`,
`--fields size,lines` returns `{file, size, lines}`, and `--fields file` means "just the paths".
`modified`, `size` and `lines` are ordinary members of the *default* set — cheap enough to always
pay for, but dropped when an explicit `--fields` does not name them.

Available fields: `file`, `modified`, `size`, `lines`, `title`, `properties`, `properties-typed`,
`tags`, `sections` (alias: `outline`), `tasks`, `links`, `backlinks`, and `all` for everything
(the pre-0.22 default shape and then some). Everything whose size scales with the document body
(`sections`, `tasks`, `links`) and the whole-vault `backlinks` lookup stay out of the default set,
but a filter that implies one still returns it on top of whatever set is in force: `--section`
adds `sections`, `--task` adds `tasks`, `--broken-links` adds `links`, `--orphan`/`--dead-end` add
`links` and `backlinks`, and `--sort links_count` / `--sort backlinks_count` add the field they
rank on. A saved view's pinned `fields` behaves exactly like an explicit `--fields`; a CLI
`--fields` replaces the pin rather than adding to it.

`title` is promoted: it has its own field and is *not* repeated inside `properties` — ask for
`--fields properties` alone to get the raw property map including `title`. Any scalar promotes,
stringified as written (`title: 42` → `"42"`, `title: 2026-08-30` → `"2026-08-30"`,
`title: true` → `"true"`); a list or map cannot, so the item's `title` falls back to the first H1,
the raw value stays in `properties.title`, and `hyalo lint` reports `HYALO007`.
`properties-typed` returns a `[{name, type, value}]` array instead of a `{key: value}` map;
`backlinks` requires scanning all files to build the link graph. Each backlink entry contains
`source` (file path), `line` (line number), and an optional `label`.

```bash
hyalo find --fields backlinks --file my-note.md       # see who links to this note (--file required: positional is PATTERN)
hyalo find --orphan                                        # find orphan files (no inbound or outbound links)
hyalo find --dead-end                                      # find dead-end files (inbound but no outbound links)
hyalo find --broken-links                                  # find files with at least one unresolved link
hyalo find --fields links --jq '[.results[].links[].kind]|group_by(.)|map({kind:.[0],n:length})'  # bucket links by kind
hyalo find --fields properties,backlinks              # combine with other fields
```

**All JSON output uses a consistent envelope:** `{"results": <payload>, "total": N, "hints": [...]}`.
`total` is present for list commands (find, lint, tags summary, properties summary, backlinks,
types list, views list, lint-rules list) — the same set `--count` accepts.
`hints` is always present (empty `[]` when `--no-hints`). `--jq` operates on the full envelope:

```bash
hyalo find --property status=draft --count                 # count matching files (bare integer)
hyalo find --property status=draft --jq '.total'           # same, via jq
hyalo find --property status=draft --jq '.results[].file'  # just file paths
hyalo summary --jq '.results.tasks.total'                  # tasks count from summary
```

**Conventions inside `results`** (so one query works across commands):

- The envelope owns `total`. Where a command repeats `total` inside `results` it means *the
  number of items that command considered* — `set`/`remove`/`append`/`properties rename`/
  `tags rename` use `total = modified + skipped`. A count of *findings* is never called
  `total`: `lint` reports `.results.violations`, `links auto` reports `.results.matched`.
- **`links auto` holds back common-word titles by default** (DEC-286). Titles that are
  ordinary English words, generic doc filenames or platform names (`index`, `README`,
  `github`, `markdown`), plus any title that dominates the run (25+ matches and 2.5% of it),
  are excluded and listed under `.results.default_excluded_titles` /
  `.default_excluded_mentions`. Setting `[links.auto] exclude_titles` hands the decision to
  your own list; `warn_common_titles = false` (or `--no-warn-common-titles`) proposes every
  candidate again.
- Top-level `results` keys are always present, including `0`, `false`, `[]` and `null`. Only
  per-item records inside arrays omit absent optional keys, so `.results.dry_run` is `false`
  (not missing) on a non-dry-run of any mutating command.
- Every mutating command whose `results` is an object reports `dry_run`, so one query answers
  "did this write?" — `hyalo madr toc --jq '.results.dry_run'`. The `apply`-style generators
  (`madr`, `okf`, `changelog`) and batch `mv` also keep an older `apply`/`applied` key that is
  always its exact inverse. `task toggle`/`task set` are the exception: they return an array of
  per-task records, and the dry-run records are the ones carrying `old_status`.
- `skipped_count` is reported by the bulk-mutation family only — `set`, `remove`, `append`,
  `properties rename`, `tags rename`:
  `hyalo set --glob '**/*.md' --property status=draft --jq '.results.skipped_count'`. A
  single-target command has no scanned-but-unchanged set, so it reports no count.
- `links fix` pairs each bucket count with a list whose suffix names the record type:
  `…_fixes` holds fix proposals (`old_target`/`new_target`/`strategy`/`confidence`),
  `…_links` holds links with no proposal.

**Hints are enabled by default.** Every query appends drill-down suggestions (`-> hyalo ...`
lines in text mode, a `"hints"` array in the JSON envelope). Read and follow these hints — they show
concrete next commands to explore deeper. Use `--no-hints` to suppress them, or `--jq` which
suppresses hints automatically.

**Check the `writes` marker before running a hint.** Read-only suggestions are prefixed `->`;
a suggestion that modifies the vault or `.hyalo.toml` is prefixed `=>` and tagged `[writes]`
(JSON: `"writes": true` on the hint object). Only the `->` ones are safe to run unattended:

```bash
hyalo find --tag rust --jq '[.hints[] | select(.writes | not) | .cmd]'
```

Pipe through `--jq` to reshape output into anything — dashboards, burndowns, reports.
`--jq` requires JSON output; piping naturally produces JSON, so `--jq` works without
an explicit `--format json` in most contexts:

```bash
hyalo find --property status=in-progress --fields tasks \
  --jq '.results | map({file, done: ([.tasks[] | select(.status == "x")] | length), total: (.tasks | length)})'
```

**Run `hyalo --help` and `hyalo <command> --help` to learn the full API.**

## Paths are vault-relative, wherever you run from

Hyalo reads `dir` from `.hyalo.toml` and resolves every file path against it. The config is
found in the current directory, or in the nearest parent directory whose configured vault
contains you — so running from the project root and running from inside the vault both work,
and both take the same vault-relative paths.

- **ALWAYS pass `--file` paths relative to the configured `dir`** (e.g.
  `iterations/iteration-17.md`). Never pass an absolute path.
- **Prefer the project root** (the directory that contains `.hyalo.toml`). It is the only
  place where the vault and the working directory cannot disagree.

Worked example:

```bash
# ✅ Right (from project root — hyalo resolves the path against `dir`)
hyalo set iterations/iteration-17.md --property status=in-progress

# ✅ Also right (from inside the vault — the parent .hyalo.toml is adopted, paths unchanged)
cd hyalo-knowledgebase && hyalo set iterations/iteration-17.md --property status=in-progress

# ❌ Wrong (absolute path — bypasses the configured `dir` entirely)
hyalo set --file /Users/me/proj/hyalo-knowledgebase/iterations/iteration-17.md --property status=in-progress
```

From a directory *deeper* than the vault root, hyalo prints a stderr note naming the config it
adopted and the vault it resolved to — the vault is wider than where you are standing. Pass
`--dir .` if you meant to scope the run to the current directory. An absolute `--file` path
still draws a correction warning: **treat it as a signal** to rewrite the path as a
vault-relative one before continuing.

## Setup (run once per project)

ALWAYS run `which hyalo` as your very first step. Do not skip this.

- **Not on PATH?** Inform the user: "The `hyalo` CLI is not installed. You can install it
  from https://github.com/ractive/hyalo." Fall back to Read/Edit/Grep/Glob.
- **On PATH?** Check for `.hyalo.toml` in the project root. If it exists, hyalo is
  configured — the `dir` setting means you don't need `--dir` on every command.
- **No `.hyalo.toml` but a directory with many `.md` files?** (e.g. `docs/`, `knowledgebase/`,
  `wiki/`, `notes/`, `content/`, or any folder with 10+ markdown files) Suggest creating one:
  ```toml
  dir = "docs"
  ```

**After confirming hyalo works**, add a line to the project's `CLAUDE.md` so future
conversations use hyalo without needing this skill:

```
Use `hyalo` CLI (not Read/Grep/Glob) for all markdown knowledgebase operations (frontmatter, tags, tasks, search). Run `hyalo --help` for usage. Output format auto-detects (text on terminals, json when piped); pass `--format text`/`--format json` to override.
```

This one-line instruction saves tokens in every future conversation.

## Moving or renaming files

When moving or renaming any file in the knowledgebase, always use `hyalo mv` — never use
system `mv`, `git mv`, or any other tool. `hyalo mv` automatically rewrites all `[[wikilinks]]`
and `[markdown](links)` across the vault that pointed to the old path. Without it, moves
silently break links throughout the knowledgebase.

```bash
# Move a file to a subfolder (updates all links vault-wide)
hyalo mv backlog/my-item.md --to backlog/done/my-item.md

# Preview what would change without writing
hyalo mv old-path.md --to new-path.md --dry-run
```

`hyalo mv` rewrites relative `.md` paths only. It leaves untouched: site-absolute links
(`/docs/...`, handled separately via site prefix), URL-scheme links (`http://`, `mailto:`),
fragment-only links (`#section`), and bare non-`.md` wiki tokens. File permissions (e.g.
`0644`) are preserved through all atomic rewrites.

## Absolute link resolution (site prefix)

Documentation sites often use root-absolute links like `/docs/guides/setup.md`. Hyalo resolves
these by stripping a **site prefix** — e.g., with prefix `docs`, the link `/docs/guides/setup.md`
becomes the vault-relative path `guides/setup.md`.

**Auto-derived by default** from the last path component of `--dir`:
- `--dir ../vscode-docs/docs` → prefix = `docs`
- `--dir /home/me/wiki` → prefix = `wiki`
- `--dir .` → prefix = name of the current directory (e.g. `wiki`)

**Override when the directory name doesn't match the URL prefix:**
```bash
# Directory is "content/" but links use "/docs/..." prefix
hyalo --site-prefix docs --dir ./content find --fields links

# Disable absolute-link resolution entirely
hyalo --site-prefix "" find --fields links
```

Also settable in `.hyalo.toml` as `site_prefix = "docs"`.
Precedence: `--site-prefix` flag > `.hyalo.toml` > auto-derived from `--dir`.
`hyalo config` reports the effective value and its source
(`flag` / `config` / `derived` / `disabled`).

## Link target resolution order

A link target is resolved against the vault in this order — the first hit wins:

1. **The path as written** — `guides/setup.md`.
2. **With `.md` appended** — `guides/setup` → `guides/setup.md`.
3. **As a directory** — `guides` → `guides/index.md`.
4. **Bare stem lookup** (wikilinks only, no `/` in the target) — `[[setup]]`
   finds `guides/setup.md` when exactly one file has that basename.

Directory-index resolution (step 3) is what makes docs corpora that publish
`foo/index.md` as the page `/foo` — MDN, GitHub Docs, Docusaurus, Hugo — read
as linked rather than 100% broken. All the site-absolute spellings work:
`/foo`, `foo` and `/foo/` reach `foo/index.md`, and `/foo#section` checks that
file's headings.

**Precedence:** a real file beats a directory index, so `foo` resolves to
`foo.md` when both `foo.md` and `foo/index.md` exist. Write the target with a
trailing slash (`foo/`) to name the directory explicitly — that flips the
order and reaches `foo/index.md`.

`hyalo backlinks foo/index.md` counts every directory spelling, and
`hyalo mv foo/index.md bar/index.md` rewrites `/foo` to `/bar` — keeping the
spelling style, never appending `.md` or injecting a site prefix the author
did not write.

## When to use hyalo vs. built-in tools

- **hyalo:** queries, frontmatter reads/mutations, tag management, task toggling, bulk updates, **moving/renaming files**, extracting sections
- **Edit tool:** body prose changes (rewriting paragraphs) that hyalo can't handle
- **Write tool:** creating brand new markdown files

Use `hyalo read` to extract file content without opening the full file:

```bash
hyalo read my-note.md                              # full body (no frontmatter)
hyalo read my-note.md --section "Tasks"            # extract one section
hyalo read my-note.md --lines 1:20                 # line range (1-based)
hyalo read my-note.md --frontmatter                # include YAML frontmatter (verbatim)
```

`--frontmatter` echoes the block's **own bytes** between its `---` fences — indentation,
quote style and comments exactly as on disk. Nothing is re-serialized on a read path. In JSON
the parsed map stays under `.results.frontmatter` and the raw text sits beside it as
`.results.frontmatter_raw` (`null` for a file with no frontmatter block).

Start with `hyalo summary` to orient yourself in a new directory (text output is the
default in interactive terminals).

`Files:` counts the notes hyalo could actually read. When some could not be, the line reads
`Files: 75 (28 skipped, 0 excluded)` — `skipped` are files whose YAML frontmatter would not
parse (see them with `hyalo lint --rule HYALO005`), `excluded` are files dropped by
`[scan] exclude`. Both are in JSON as `results.files.skipped` / `results.files.excluded`, with
per-directory attribution under `results.files.directories[].skipped`.

Every scanning command reports unusable files as **one** stderr line
(`warning: skipped N files with unparsable frontmatter …`) rather than one YAML excerpt per
file. `-q` silences it; `[scan] verbose_skips = true` or `RUST_LOG=hyalo=debug` brings the full
per-file diagnostics back.

## Available commands — read the CLI's own help first

`hyalo -h` lists every command grouped by intent (read / write / config), one line each,
with the capability families each one covers. `hyalo <cmd> -h` is the short page for one
command; `hyalo <cmd> --help` is its full syntax reference — property operators, sort keys,
`--fields` values, output shapes, and a cookbook. Both are generated from the binary you
are actually running, so they cannot drift the way a copy in this file can. Read them
before guessing a flag, and before falling back to `grep`.

```bash
hyalo -h                 # every command, grouped, with composed examples
hyalo find -h            # one screen: filters, output flags, examples
hyalo find --help        # every operator, sort key, field name and recipe
```

What follows is only what those pages do not say — the behaviour that surprises people.

### Pitfalls

- **`mv` has two modes.** Single-file mode writes immediately (`--dry-run` to preview;
  `--apply` is rejected there). Batch mode (`--glob`/`--property`/`--tag`/`--type`)
  defaults to dry-run and needs `--apply` to commit.
- **`links fix` withholds low-confidence repairs.** Fuzzy hits and basename fallbacks are
  reported separately and excluded from `--apply` unless you pass `--apply-fuzzy`, which is
  gated again by a confidence floor (0.8 default; `--min-confidence <0.0-1.0>` or
  `[links] fuzzy_min_confidence`). Confidence weights the final path segment at 70% and the
  directory path at 30%. A repair is written in the form the link was written in, and one
  whose emitted target would still not resolve is refused and reported under `unfixable`.
  Targets normalizing above the vault root count as `out_of_vault`, not `broken`.
  Destinations containing `{%`, `{{` or `${` are template expressions, counted under
  `templated` and never rewritten. A target with an explicit non-`.md` extension is never
  matched against a `.md` note (DEC-266), so a broken `Companies.base` is honestly unfixable
  rather than rewritten into a note link, and no candidate is ever reported at confidence 0.0.
- **`append` does not accept `--tag`.** Tags are scalar list items; use `set --tag`.
- **`--where-property` / `--where-tag` default to all `**/*.md`** when neither `--file` nor
  `--glob` is given. Always pair a bulk mutation with `--dry-run` first.
- **Bare `properties` / `tags` / `types` / `views` / `lint-rules` run their `summary`
  (or `list`) action** — there is no separate command to remember. `--index` is accepted on
  the bare form as well as on the subcommand (`hyalo properties --index` ==
  `hyalo properties summary --index`).
- **`properties rename` renames the key in place** — it keeps its position in the block and
  the value's exact source text (quoting, spacing, comments, list indentation), and an empty
  `rating:` becomes an empty `score:`, never `score: null`.
- **`tags rename` on a parent moves the whole subtree** (Obsidian semantics): `--from music
  --to audio` also renames `music/genres`, works when only children exist, and never matches
  `musical` (the match needs a `/` boundary). `results.renamed_tags` lists every tag it
  actually touched with its file count.
- **`lint` exits 1 when errors are found**, which is what makes it usable as a CI gate;
  `--strict` promotes missing-type and undeclared-property warnings to errors.
- **Every link carries a `kind`** (`--fields links`): `wikilink`, `embed` (`![[…]]`),
  `markdown`, `external` (any `scheme:` URI — `https:`, `obsidian://`, `mailto:`, `file://`)
  or `attachment` (resolved to a non-`.md` vault file: an image, a PDF, an Obsidian `.base`).
  `![alt](img.png)` is an embed too (DEC-297), so a missing image is visible. A same-file anchor
  (`[[#H]]`, `[text](#frag)`) keeps the syntax it was written in — an anchor-only *markdown*
  link is `kind: "markdown"` and carries its link text.
  `external` and `attachment` are **never broken** — they stay out of `find --broken-links`,
  `summary.links.broken` and HYALO006, and are not graph edges for `--orphan` / `--dead-end`.
  So bucket broken links with
  `select(((.kind == "external" or .kind == "attachment") | not) and ((.path == null and (.out_of_vault | not)) or .broken_anchor))`.
- **Frontmatter `aliases:` resolve wikilinks** (DEC-296): `[[Leah]]` finds the note declaring
  `aliases: [Leah]` (list or bare string). A filename or path always wins; an alias claimed by
  two notes is ambiguous, not resolved; matching folds case like DEC-267; `[[alias#Heading]]`
  and `[[alias|label]]` work. The link's `kind` stays `wikilink` and it carries `via: "alias"`.
  Alias links are real graph edges (`backlinks`, `--orphan`, `--dead-end`, `summary.links`,
  HYALO006) and `links fix` never proposes or fuzzy-matches a rewrite for one. `mv` leaves them
  alone — the alias travels with the note. Opt out with `[links] aliases = false`
  (`hyalo config --jq '.results.links.aliases'`).
- **Link resolution folds case on every platform** (DEC-267), so `[[AidenLx]]` resolves to
  `People/aidenlx.md` whatever the filesystem does. Opt out with `[links] case_insensitive =
  "false"`. `links fix --case-insensitive` no longer changes what resolves; it only hides the
  cosmetic `link-case-mismatch` rewrite plans. `hyalo config --jq
  '.results.links.case_insensitive'` reports the effective mode. `false` disables hyalo's
  case-folding index only: the literal path probe that runs first belongs to the filesystem, so
  on a case-insensitive volume the link still resolves — to the canonical on-disk path.
  Exact-match resolution is guaranteed only on a case-sensitive filesystem.
- **A `site_prefix` link is never a case mismatch** (DEC-295): a site-absolute target carrying
  the configured prefix (`/en-US/docs/Web/CSS/Guides/Anchor_positioning`) is written in the
  site's own URL convention, not the on-disk folder casing, so `links fix` produces no case plan
  for it — on a copy of MDN's CSS tree that rule had proposed 5096 rewrites across 1049 files.
  Broken site-absolute links are still fixed, and every rewrite **keeps the incoming form**: a
  directory link stays a directory link (trailing slash included), an authored `.md` stays
  `.md`, and neither `/index` nor `.md` is appended to a form that lacked it.
- **A dead `#anchor` that prefixes exactly one heading gets a `suggested_fragment`** (DEC-268):
  `[[decision-log#DEC-068]]` reports `"DEC-068: Snapshot index format"` as the text to write.
  Reported, never applied — an ambiguous prefix suggests nothing. `-`, `_` and a space are one
  character class for that prefix test (DEC-298), so `#Browser_compatibility` suggests the
  `Browser compatibility` heading.
- **`links fix` reports the string it will write** (iter-272): every plan carries
  `emitted_target` beside the vault-relative `new_target`, filled by the same planning pass in
  `--dry-run` and `--apply`, so a preview is byte-accurate about what lands on disk.
- **`config` reports a broken config rather than failing.** `results.malformed` /
  `results.parse_error` mean every other value shown is a built-in default.
- **`views run <name>` is exactly `find --view <name>`** — same merge rules, same output.

## Schema & Lint

`hyalo lint` runs two passes in one invocation:

1. **Frontmatter** — validates against the `[schema]` block in `.hyalo.toml`. No-op when no schema is configured.
2. **Markdown body** — stock mdbook-lint rules (MD001..MD059) plus the HYALO native rules:
   - **HYALO001** — bare `[]` should be `- [ ]` (autofixable)
   - **HYALO002** — `status: completed` requires all task checkboxes ticked (fires only when `[schema.types.*].properties.status` is declared as an enum containing `"completed"`)
   - **HYALO005** — frontmatter that cannot be parsed (invalid YAML, duplicate keys, oversized scalar). Error by default and the file still counts in `files_checked`, so a corrupt file fails CI instead of vanishing silently. Severity is configurable via `[lint.rules.HYALO005]` but no profile downgrades it.

```bash
hyalo lint                               # whole vault, summary mode
hyalo lint iterations/iter-42.md         # one file
hyalo lint --fix --dry-run               # preview autofixes
hyalo lint --fix                         # apply
```

**Obsidian grammar (autofix safety).** Four stock rules are narrowed so `--fix` cannot
corrupt a vault; `hyalo lint-rules show <ID>` states each deviation:

- **MD018** exempts tag lines — a single `#` plus a tag token is `#todo`, not a heading
  missing its space. `##Heading`, `#1`, and a capitalized word followed by prose
  (`#Heading typo`) still fire.
- **MD034** ignores URLs that already sit inside link markup (link/image destination,
  autolink, wikilink, reference definition).
- **MD042** accepts an image as link text (`[![](img.png)](https://…)`).
- **MD001** reports a skipped heading level but is **not autofixable**: renumbering a
  deliberate `######` caption rewrites authored structure. Silence the warning with
  `hyalo lint-rules set MD001 --enabled false`.

**Code blocks are content, not defects (iter-271).** A rule that lints prose does not fire on
a line inside a fenced (```` ``` ```` / `~~~`) or indented code block, or inside an HTML
comment — MD019 used to rewrite `#   three` inside a ```` ```text ```` sample. The exceptions
are the rules whose subject *is* the block: **MD031/MD040/MD046/MD048** (the fence itself),
**MD047** (the file's final newline) and **MD010** (a hard tab in a sample is a real
portability problem, as markdownlint also holds). **MD031** additionally stays quiet at the
opener of a fence that never closes, where its "blank line after" would land inside the sample.

**Suppression comments (DEC-294).** markdownlint's own directives are honoured, taking rule
ids or aliases, case-insensitively; with no ids a directive covers every rule, HYALO ones
included:

```markdown
<!-- markdownlint-disable no-hard-tabs -->
	a sample whose point is the tab
<!-- markdownlint-enable no-hard-tabs -->

<!-- markdownlint-disable-next-line MD009 -->
a line that keeps its trailing spaces   
```

`-disable-line`, `-disable-file` and `-enable-file` work too; `-capture`/`-restore` are not
supported, and MDN's `-nolint` info-string suffix is not markdownlint syntax and is ignored.

When two fixes want the same bytes one is deferred and reported as a conflict;
`--fix` text output names it as `conflict <RULE> line <N>: range overlap with <RULE>`
(first 20 per file, `--detailed` for all).

Use `hyalo lint --help` for narrowing flags (`--rule`, `--rule-prefix`, `--detailed`, `--max-per-rule`, `--fix-rule`, etc.). The snapshot index does **not** accelerate the body pass.

**Strict mode:** `hyalo lint --strict` (or `[lint] strict = true` in `.hyalo.toml`)
promotes the "no `type` property" and "undeclared property in frontmatter" warnings to
errors, so lint exits non-zero on those cases. Useful in CI and `/hyalo-tidy` to fail
fast on schema drift.

**GitHub PR annotations:** `hyalo lint --strict --format github` (lint-only) emits
`::error`/`::warning file=…,line=…,title=<RULE_ID>::<message>` GitHub Actions workflow
commands so violations render as inline PR annotations, plus a one-line summary. Paths are
repo-root-relative, so run it from the repository root. Composes with `--files-from -` for a
diff-aware variant. Annotations are never truncated (the display caps are lifted for github).
Under `--fix --dry-run --format github`, would-be-fixed violations become `::notice` with a
`[fixable]` title prefix and the summary reads `N fixable, M remaining`. Other subcommands
reject `--format github`.

**Skip visibility & unlimited output:** with `--files-from`, dropped input paths surface as a
`note:` line (`--format text`, stderr) or a `::notice::` (`--format github`), so a diff-scoped
run always shows what it skipped without `jq`. `--limit 0` on lint means **unlimited** (lift
the file cap) — the `errors`/exit code always reflect the whole vault, never just the shown slice.

**Tune which rules run with `hyalo lint-rules`** (list / show / set / remove). Reach for it when a rule is too noisy on your KB style — disable it or change its severity rather than living with the warnings:

```bash
hyalo lint-rules list                          # see what's enabled
hyalo lint-rules set MD013 --enabled false     # turn one off
hyalo lint-rules set HYALO001 --severity error # promote to error
```

Lint also warns about comma-joined tags (e.g. `tags: ["cli,ux"]` instead of two list
items); `--fix` splits them into proper list entries automatically.

Lint additionally validates saved views in `.hyalo.toml`: if a `[views.*]` entry only
sets `fields` (which controls output columns, not which files match), lint flags it so
you can add a real filter like `orphan = true` or `tag = [...]` (saved views
store tags under the `tag` key).

Exit codes: 0 = clean, 1 = errors found, 2 = internal error.

**Schema format:**

```toml
[schema.default]
required = ["title"]

[schema.types.iteration]
required = ["title", "date", "status", "branch", "tags"]

[schema.types.iteration.properties.status]
type = "enum"
values = ["planned", "in-progress", "completed", "superseded"]

[schema.types.iteration.properties.date]
type = "date"

[schema.types.iteration.properties.branch]
type = "string"
pattern = "^iter-\\d+/"
```

Property types: `string` (optional `pattern` regex), `date` (YYYY-MM-DD), `datetime` (naive YYYY-MM-DDThh:mm:ss, no offset), `datetime-tz` (timezone-aware RFC 3339: YYYY-MM-DDThh:mm:ss plus `Z` or `±hh:mm`, e.g. `2026-05-28T22:44:47+00:00`), `number`, `boolean`, `list`, `string-list` (optional `item_pattern` regex), `object-list` (list of maps; see below), `enum` (with `values`). `datetime` and `datetime-tz` are disjoint — a naive value never satisfies `datetime-tz` and vice-versa.

**`object-list`** describes a list whose items must all be YAML maps, with three flat per-key keys: `required-keys` (present in every item), `allowed-keys` (the complete key set; omit to allow extras), and a `key-patterns` table mapping key → regex applied to that key's scalar value:

```toml
[schema.types.memory.properties.sources]
type = "object-list"
required-keys = ["ref"]
allowed-keys = ["ref", "commit", "version", "updated", "read"]

[schema.types.memory.properties.sources.key-patterns]
ref = "^(github|jira|slack|decision):|^https?://"
commit = "^[0-9a-f]{7,40}$"
```

Lint reports every violating item independently, naming the 0-based item index and the key (`property "sources" item 1: unknown key "rev" (allowed: ...)`); a leftover plain-string item gets a `did you mean \`- ref: <value>\`?` fix-it hint, because `find --property sources.ref=…` skips scalar items and would otherwise never report it. Every `key-patterns` regex is compiled when `.hyalo.toml` loads, so a bad regex is a `schema/malformed` error, not a per-file message. `object-list` is **config-only**: `types set --property-type` rejects it (as it does `string-list`) and `--fix` has no fixer, so its violations report `autofixable: false`.

Reserved-file exemption: `[schema] exempt = ["**/index.md", "**/log.md"]` binds matching files to no schema (they skip missing-`type`, required-property, and undeclared-property checks). Globs are vault-relative and cross-platform.

**`required` empty-value semantics:** a required property whose value is YAML null (`tags: ~`) or an empty array (`tags: []`) is an error (`required property "tags" must not be empty`). Vacuous values convey no information for a required field, so they're treated as semantically equivalent to absent. This fires regardless of declared constraint type. Atomic-typed required properties (`string`, `date`, `number`, ...) only need to be present — an empty string or zero still satisfies them. So `required = ["tags"]` + `type = "list"` is the idiomatic way to enforce non-empty tags; no separate `min_items` knob exists.

When no `[schema]` block exists, lint exits 0 with zero violations (backwards compatible).

`hyalo summary` includes a `schema` field with error/warning counts when a schema is configured.

**Validate on write:** `hyalo set` and `hyalo append` accept `--validate` to reject values
that would fail lint. Enable globally via `[schema] validate_on_write = true` in `.hyalo.toml`.

**A `--validate` write refuses when the schema itself is broken (DEC-290).** If `[schema]`
is present but cannot be loaded — an uncompilable regex, a key on the wrong property type —
the schema falls back to empty, so validating against it would reject nothing. `set` /
`append` therefore exit 1 and write nothing whenever validation was asked for (`--validate`
or `validate_on_write`), `--dry-run` included. The same write *without* `--validate` still
proceeds, and `mv`, `remove`, `task toggle` and every read are unaffected — fix `[schema]`
(`hyalo lint` reports it as `schema/malformed`) or drop `--validate` for that one write.

**Ignore known-bad files:** add `[lint] ignore = ["legacy/known-bad.md", "vendor/**/*.md"]`
to `.hyalo.toml` to skip listed files during `hyalo lint` (plain strings match literally;
glob meta-characters use `--glob` semantics). Read-only commands still count them among the
files they skipped.

**Naming a file overrides the ignore list (DEC-284).** A path given positionally, with
`--file`, or through `--files-from` is linted even when `[lint] ignore` matches it — naming a
file is a stronger signal than a glob written once in `.hyalo.toml`. `--glob` and the bare
vault sweep keep honouring the list. So `git diff --name-only | hyalo lint --files-from -`
lints changed ignored files (what a diff gate wants); select paths with `--glob` when you
*do* want the ignore list applied.

**Exclude a tree from the whole tool:** `[lint] ignore` narrows one command. `[scan] exclude =
["Templates/**"]` is the vault-wide knob — hyalo's analogue of Obsidian's "Excluded files".
Matching files are dropped at discovery, so `find`, `summary`, `tags`, `properties`, `lint`,
`links *`, `mv`, `backlinks`, `create-index`, `views`, `types`, `okf` and `madr` all see the
same vault, and `--index` reads drop them too (no rebuild needed after changing the list).
Naming an excluded file explicitly (`--file Templates/x.md`) is **refused** with the matching
glob, never silently skipped. `hyalo config` reports the effective list under
`results.scan.exclude`.

**A broken `.hyalo.toml` fails a gate:** `lint`, `find --strict` and `views run` exit 1 when the
config does not parse, because their exit code is a verdict and a verdict computed without the
config's `[lint] ignore` and schemas is not the one the vault asked for. Other reads still
answer, with a warning `-q` cannot suppress.

`hyalo lint --count` returns just the number of files with violations.

## Types — manage type schemas

`hyalo types` manages `[schema.types.*]` entries in `.hyalo.toml` without hand-editing TOML. All mutations preserve existing comments and formatting.

```bash
hyalo types list                                     # list all defined types
hyalo types show iteration                           # full merged schema for a type
hyalo types remove iteration                         # remove a type entry
hyalo types set iteration --required title,date      # create or update type (upsert)
hyalo types set iteration --default "status=planned" # set default (auto-applies to vault files)
hyalo types set iteration --property-type "date=date"
hyalo types set iteration --property-values "status=planned,in-progress,completed"
hyalo types set iteration --filename-template "iterations/iteration-{n}-{slug}.md"
hyalo types set iteration --required branch --dry-run  # preview without writing
```

`types set` is an upsert — it auto-creates the type if it doesn't exist. When adding `--required` fields, a property constraint is auto-created for fields without an explicit one; its **type is inferred from the values the vault already holds** for that key on files of this type (`string` when there are none), so declaring a list-valued property required does not instantly make every file violate it.

**How a file binds to a type** (DEC-281): `type:` may be a plain string, a `[[Wikilink]]` (bare or quoted; an alias or a path resolves to the note name), or a **one-element list** of either — the shape Obsidian's property editor writes for a link-typed property. `type: ["[[Authors]]"]`, `type: "[[Authors]]"` and `type: Authors` all bind to `Authors`. A multi-element list names no type and `lint` reports it.

When `--default` is used, hyalo applies the default to all vault files of that type missing that property.

**Scaffolding a file from a type:** `hyalo new --type <name> --file <path>` writes the type's
required properties and required sections. `--dry-run` prints the scaffold (JSON adds
`content`) and writes nothing — not even the parent directory. The placeholders are
deliberately un-fillable rather than plausible (DEC-285): a required `string` gets `TBD`, and
a required `number` / `date` / `datetime` / `boolean` with no schema `default` is written
**empty** (`rating:`), which `hyalo lint` reports as "required property must not be empty".
A `default` declared in the schema (including `$today`) is emitted verbatim. `new` writes
only what the schema declares — chain `hyalo set` for anything else.

```bash
hyalo new --type iteration --file iterations/iteration-99-x.md --dry-run   # preview
hyalo new --type note --file notes/draft.md && hyalo set notes/draft.md --property status=draft
```

## Views — saved find queries

Views save frequently-used filter combinations under a name in `.hyalo.toml`.
They compose: CLI flags passed alongside `--view` extend or override the saved filters.

**Before constructing a complex `hyalo find`, check if a matching view exists:**
```bash
hyalo views list
```

**If you run the same multi-filter find command 3+ times, save it as a view:**
```bash
hyalo views set stale-iterations --property type=iteration --property status=in-progress
hyalo views set perf-research "performance" --tag research   # BM25 pattern + filter
hyalo views set orphans --orphan                             # files with no inbound/outbound links
hyalo views set dead-ends --dead-end                         # files with inbound but no outbound links
hyalo find --view stale-iterations                    # reuse later
hyalo find --view stale-iterations --limit 5          # compose with overrides
hyalo views run stale-iterations --limit 5            # same query, view-first spelling
hyalo views run perf-research "cache eviction"        # positional PATTERN overrides the saved one
```

hyalo suggests saving non-trivial queries as views in its hint output — follow those hints.

**Manage views:**
- `hyalo views list` — show all saved views
- `hyalo views set <name> [filters...]` — create or update a view
- `hyalo views remove <name>` — delete a view
- `hyalo find --view <name> [extra filters...]` — use a view, optionally with overrides
- `hyalo views run <name> [PATTERN] [extra filters...]` — the same query, spelled view-first

## Output format

Output format is auto-detected — `text` for interactive terminals, `json` when piped.
Pass `--format text` or `--format json` to override, or set a default in `.hyalo.toml`
(`format = "text"` / `format = "json"`). An explicit `--format` flag always wins.

`text` is the compact, low-token format designed for LLM consumption — less noise than
JSON, fewer tokens. Use it when orienting yourself or scanning results.

**`init` and `deinit` are the exception to auto-detection.** Their summary is a human
progress report, so it stays text even when piped; pass `--format json` (or `--jq`) to get
`{results: {command, root, actions: [{action, target, detail?}], notes?}, hints, dir}`
instead. `--dir` scopes them like every other command: naming a tree outside the current
directory initializes — or cleans — *that* tree, not this one.

**`--format text` and `--jq` are mutually exclusive.** `--jq` operates on JSON, so it
requires JSON output. Piping naturally produces JSON, so `--jq` works without an
explicit flag in most contexts. If you need to filter/reshape output, just pipe through
`--jq`. If you want a readable overview, rely on the auto-default (or pass
`--format text` explicitly when piping to a pager).

## The backlinks command

Use `hyalo backlinks <path>` to find all files that link to a given file (reverse link
lookup). This builds an in-memory link graph by scanning all `.md` files in the directory,
detecting both `[[wikilinks]]` and `[markdown](links)` in body content *and*
`[[wikilinks]]` in **every** frontmatter value — a scalar (`type: "[[Author]]"`), a
list (`categories: ["[[Books]]"]`), or a nested map, at any depth. Each such entry
comes back with `kind: "frontmatter"`, the `property` it was written under, and the
frontmatter line it sits on. Set `[links] frontmatter = false` in `.hyalo.toml` to
narrow the scan back to the four legacy link properties (`related`, `depends-on`,
`supersedes`, `superseded-by`), or `[links] frontmatter_properties = [...]` to name
your own list. The file can be passed positionally or with `--file`.

```bash
# Which files reference iteration-37?
hyalo backlinks iterations/iteration-37-bulk-mutations.md

# JSON output for programmatic use
hyalo backlinks iterations/iteration-37-bulk-mutations.md --format json
```

Supports `--format text` (compact), `--format json`, and `--limit N` (default: 50,
use `--limit 0` for all). Format auto-detects when not passed. Useful for impact
analysis (what depends on this file?), finding orphan pages, and navigating link
structure.

## Default output limits

Capped commands (`find`, `lint`, `tags summary`, `properties summary`, `backlinks`) return at
most **50 results** by default to avoid flooding the context window. When results are truncated,
output shows "showing N of M matches" and a hint to get all results.

`types list`, `views list` and `lint-rules list` emit a `total` (so `--count` works) but are
*not* capped and reject `--limit` — they enumerate small fixed catalogs and always return
everything.

- `--limit N` — override the default (e.g. `--limit 20` for fewer, `--limit 200` for more)
- `--limit 0` — unlimited output (returns everything)
- `--count` — just get the total count without any results

The default can be changed in `.hyalo.toml`:
```toml
default_limit = 100   # 0 = unlimited
```

## Snapshot index — ALWAYS create for vaults with 500+ files

**For any vault with more than ~500 files, ALWAYS create a snapshot index before running
queries.** The index makes property/tag queries 10-15x faster (e.g. ~80ms vs ~1.5s on a
14K-file vault). Without it, every query scans every file from disk.

**Rule of thumb:** run `hyalo summary` first. If it reports more than 500 files,
immediately create an index before proceeding with any analysis.

```bash
# Step 1: Check vault size
hyalo summary

# Step 2: Create index if >500 files (one scan, reused by all subsequent queries)
hyalo create-index

# Step 3: Use --index on ALL subsequent commands (defaults to .hyalo-index in vault dir)
hyalo find --property status=in-progress --index
hyalo summary --index
hyalo tags --index          # also `tags summary --index`
hyalo properties --index    # also `properties summary --index`
hyalo backlinks some-note.md --index

# Mutations also work with --index — they patch the index after each write
hyalo set note.md --property status=completed --index
hyalo task toggle note.md --line 5 --index

# Drop the index when done
hyalo drop-index
```

The index is **ephemeral** — create it, use it, drop it within the same session. Never persist
it across sessions.

**Index-aware mutations:** all mutation commands (`set`, `remove`, `append`, `task`, `mv`,
`tags rename`, `properties rename`) support `--index`. They still read/write individual files
on disk, but after each mutation they patch the in-memory index entry and save the snapshot
back — keeping it current for subsequent queries. This is safe as long as **no external tool
modifies files in the vault** while the index is active. If only hyalo touches the files,
the index stays consistent across interleaved reads and writes.
