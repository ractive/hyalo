---
paths:
  - "hyalo-knowledgebase/**"
---
Prefer `hyalo` CLI for operations on files in this directory:
- **Read the CLI's own help before guessing a flag**: `hyalo -h` lists every command grouped by
  intent, one line each; `hyalo <cmd> -h` is one screen for one command; `hyalo <cmd> --help` is the
  full syntax reference (operators, sort keys, `--fields` values, output shapes, cookbook).
- **Empty result sets are self-documenting**: a `find` that matches nothing echoes the filters it
  applied and hints at the next step (did-you-mean over the values the property really has, and the
  same query with its most selective filter dropped). When the empty query filtered on a property
  regex (`--property 'title~=/DEC-25/'`) whose pattern *does* occur in body prose, the hints lead
  with the equivalent body search (`hyalo find -e 'DEC-25'`) — read them before re-guessing the
  filter, because identifiers that live in `##` headings are never frontmatter titles.
- **Search/filter**: `hyalo find --property status=planned --tag iteration`
- **Body search**: `hyalo find "broken links"`
- **Title regex**: `hyalo find --property 'title~=link'`
- **Inspect config**: `hyalo config` — shows effective dir, config path, hints, format, site_prefix,
  the `[links.auto]` auto-link settings, and `links.fuzzy_min_confidence` (the confidence floor
  `links fix --apply-fuzzy` applies). `--raw` adds the file's text; `results.malformed` /
  `results.parse_error` flag a config that exists but does not parse.
  JSON uses the standard envelope: `hyalo config --jq '.results.dir'`
- **`results` key conventions**: the envelope owns `total`; inside `results`, `total` always means
  "items the command considered", so a count of findings gets its own name (`lint` →
  `.results.violations`, `links auto` → `.results.matched`). Top-level `results` keys are always
  present — `0`, `false`, `[]` and `null` included — so `.results.dry_run` is `false`, not
  missing, on any non-dry-run mutation; only per-item records inside arrays omit optional keys.
  Every mutating command whose `results` is an object reports `dry_run` (the `apply`-style
  generators — `madr`, `okf`, `changelog` — and batch `mv` also keep their older `apply`/`applied`
  key, which is always its exact inverse). `skipped_count` is reported by the bulk-mutation family
  only — `set`, `remove`, `append`, `properties rename`, `tags rename` — because a single-target
  command has no scanned-but-unchanged set. `task toggle`/`task set` return an array of per-task
  records with no top-level object: their dry-run records carry `old_status`, applied records do
  not. `init`/`deinit` and `create-index`/`drop-index` write config/index rather than notes and are
  outside this contract. `init`/`deinit` still answer `--format json` (and `--jq`) with their own
  minimal envelope — `results.command`, `results.root`, `results.actions[]` of
  `{action, target, detail?}`, plus a hoisted top-level `dir` for `init` — but stay text when
  merely piped, because their summary is a progress report, not a result set.
- **Config discovery**: `.hyalo.toml` is read from the current directory, or from the nearest
  parent whose configured vault contains it — running from inside the vault keeps the config.
- **`init`/`deinit` follow `--dir` too**: a vault at or below the current directory leaves the
  project root at CWD (`init` records `dir` relative to it, `deinit` cleans CWD); a `--dir` naming
  a tree *outside* CWD moves the whole operation into that tree — `init` writes its `.hyalo.toml`
  there with `dir = "."`, `deinit` removes that tree's integration files and never CWD's. Both
  lead their summary with a `target <path>` line whenever the root is not CWD.
- **`--dir` is a vault, not a config**: `--dir <configured-vault>` keeps `.hyalo.toml` in effect
  (the flag is just redundant); `--dir <other-tree>` switches to that tree's own `.hyalo.toml` — or
  built-in defaults — and says so on stderr. A `.hyalo.toml` that fails to parse blocks every
  mutating command **and every gate command** (`lint`, `find --strict`, `views run`) with
  exit 1; other reads continue on defaults, with a `-q`-proof warning.
- **A project-local `dir` must stay at-or-below the config directory**: an absolute `dir` or one
  whose `..` components net above where `.hyalo.toml` lives refuses *every* command (reads
  included) with a `-q`-proof error naming the file and value — `hyalo config` still reports it
  (`dir_out_of_bounds`) rather than being refused. Pass `--dir` explicitly if that wider scope is
  genuinely intended; an in-bounds relative `dir`, including a bounded `sub/../kb` round-trip, is
  unaffected.
- **`[scan] exclude` is the vault-wide exclusion knob** (DEC-277, iter-265):
  `[scan] exclude = ["Templates/**"]` in `.hyalo.toml` drops matching files at discovery, so
  every command — `find`, `summary`, `tags`, `properties`, `lint`, `links *`, `mv`,
  `backlinks`, `create-index`, `views`, `types`, `okf`, `madr` — and every `--index` read
  sees the same vault. The narrower per-feature lists (`[lint] ignore`, `[okf] ignore`,
  `[schema] exempt`) still apply within what survives. An explicitly named excluded file
  (`--file Templates/x.md`) is **refused**, naming the glob, rather than silently skipped.
  `hyalo config` reports the effective list as `results.scan.exclude`.
- **Unusable files are summarised, not spelled out** (DEC-278, iter-265): a file whose YAML
  frontmatter will not parse is skipped and counted, and the run ends with one stderr line —
  `warning: skipped N files with unparsable frontmatter (run hyalo lint --rule HYALO005 for
  details)`. `-q` silences it; `[scan] verbose_skips = true` or `RUST_LOG=hyalo=debug` restores
  the per-file YAML excerpts. `summary` accounts for them: `Files: 75 (28 skipped, 0 excluded)`
  in text, `results.files.skipped` / `results.files.excluded` in JSON, with per-directory
  attribution under `results.files.directories[].skipped`.
- **A broken `.hyalo.toml` fails a gate** (DEC-279, iter-265): `lint`, `find --strict` and
  `views run` exit 1 when the config does not parse, because a caller acts on their exit code
  and a verdict computed without the config's `[lint] ignore` and schemas is not the vault's.
  Every other read still answers, with the `-q`-proof warning.
- **Hints marked `[writes]`** (`=>` prefix in text, `"writes": true` in JSON) modify the vault or
  `.hyalo.toml`; `->` hints are read-only and safe to run unattended.
- **Read frontmatter/metadata**: `hyalo find --file <path>`, `hyalo properties`, `hyalo tags`
- **`--index` reaches the bare group** (iter-266): `hyalo properties --index` and
  `hyalo tags --index` are accepted just like `hyalo properties summary --index`, so every
  reading command takes the flag the same way.
- **Renames touch only what changed** (iter-266): `properties rename --from rating --to score`
  rewrites the key token where it stands — same position in the block, same value bytes
  (quoting, spacing, comments, list indentation), and an empty `rating:` becomes an empty
  `score:`, never `score: null`. `tags rename --from music --to audio` renames the parent
  **and its whole subtree** (`music/genres` → `audio/genres`, DEC-282), proceeds when only
  children exist, and never matches `musical` — the match needs a `/` boundary.
  `results.renamed_tags` lists every tag actually renamed with its file count.
- **A `type:` may be a string, a `[[Wikilink]]`, or a one-element list of either**
  (DEC-281): `type: ["[[Authors]]"]`, `type: "[[Authors]]"` and `type: Authors` all bind to
  the `Authors` schema, so `types set Authors …` applies and `set --validate` refuses a
  violating value (exit 1, `--dry-run` included). A multi-element list names no type and
  `lint` reports it. `types set --required K` auto-declares K's constraint with the type
  **inferred from the vault's existing values** for K, not a hardcoded `string`.
- **`summary` keys properties by name** (iter-266): a property with more than one type across
  the vault is ONE row reading `published (103: 79 datetime, 24 date)`, so
  `.results.properties | length` equals `hyalo properties --count`.
- **`read --frontmatter` is byte-exact**: text mode echoes the block between its `---` fences
  verbatim (no YAML re-serialization); JSON keeps the parsed map and adds `frontmatter_raw`.
- **`find` results are compact by default**: every item carries `file`, `modified`, `size`
  (bytes), `lines`, `title`, `properties` and `tags`. `sections`, `tasks`, `links`, `backlinks`
  and `properties-typed` come only from `--fields` (or `--fields all`) — or automatically from the
  filter that implies them (`--section`, `--task`, `--broken-links`, `--orphan`, `--dead-end`,
  `--sort links_count|backlinks_count`). `title` is promoted out of `properties`, so read it as
  `.results[].title`, not `.results[].properties.title`.
- **`title` is promoted in three steps** (DEC-283, iter-267): a scalar frontmatter `title`, else
  the first H1, else the **filename stem** — Obsidian's own fallback, so a vault whose notes carry
  no `title` property still sorts and filters usefully instead of reporting `title: (none)`.
  `title_source` (`property` | `h1` | `filename`) rides along with `title` and says which step
  answered, so a consumer can tell an authored title from a derived one. `--title`,
  `--property 'title~=…'` and `--sort title` all read the promoted value; test the raw
  frontmatter key with `--property title` / `--property '!title'`.
- **Sort direction is uniform** (DEC-273, iter-264): every `--sort` key orders ascending and
  `--reverse` inverts it, so `--sort backlinks_count --reverse` is "most linked first" just as
  `--sort modified --reverse` is "newest first". `score` alone ranks best-match-first. A file
  whose sort property is missing or null always sorts last, in both directions.
- **Null, empty list and absent are three different things** (DEC-274, iter-264): `!K` is absent,
  `K=null` is present with a YAML null (`~`, `null`, an empty value), `K=[]` is present and an
  empty list; `K!=null` / `K!=[]` are their present-and-not forms. `aliases: [null]` matches none
  of them. Ordering ops are typed — numbers compare numerically (`rating>=6` matches
  `rating: "7"`), ISO dates by date, plain strings as text, and a value of any other kind never
  matches, so `last>=2023-09-01` skips `last: "[[2022-04]]"`.
- **The regex operator is `~=`, never `=~`** (DEC-276, iter-264): `K=~/pat/` is a hard error
  naming `~=` (it used to be read as equality against the literal `~/pat/`, which matched every
  YAML null). An empty pattern (`K~=`, `K~=//`) and an empty selection (`--fields ''`) are errors
  too — all exit 1.
- **`find`'s `results` is always the array of files**, whichever way the file list was supplied
  (`--file`, `--glob`, `--files-from`, or a full scan), so `.results[0]` always works. The
  `files_missing` / `files_skipped_non_md` / `files_skipped_outside_vault` counters are top-level
  envelope keys beside `total` and `hints`, present on every `find` and zero when `--files-from`
  was not used. In JSON, `--fields properties-typed` lands under `properties_typed`; both
  spellings are accepted on the flag.
- **`--fields` is an exact projection**: without it you get the seven default keys above; with it
  you get exactly the fields you named plus `file`, the one key that is never dropped. So
  `--fields title` returns `{file, title}` and `--fields size,lines` returns
  `{file, size, lines}` — `modified`/`size`/`lines` are ordinary members of the default set, not
  structural. `--fields file` means "just the paths"; a filter still adds what it needs on top; a
  view's pinned `fields` behaves like an explicit `--fields`, and a CLI `--fields` replaces the pin.
- **A non-string `title` still works**: any scalar promotes, stringified as written — `title: 42`
  → `"42"`, `title: 2026-08-30` → `"2026-08-30"`, `title: true` → `"true"` — and the typed value
  stays under `--fields properties-typed`. A list or map `title` cannot promote: the item's `title`
  falls back to the first H1 and the raw value stays in `properties.title`, with `HYALO007`
  reporting it.
- **Check `size`/`lines` before reading**: both appear on `find` items and on `read` results, so a
  large file can be taken in slices — `hyalo read <path> --lines 1:80` or `--section "Heading"` —
  instead of whole.
- **Read content/sections**: `hyalo read <path>` or `hyalo read <path> --section "Heading"`
- **Mutate frontmatter**: `hyalo set`, `hyalo remove`, `hyalo append`
- **Auto-link**: `hyalo links auto --first-only --exclude-target-glob 'templates/*' --apply`.
  Persist the noisy-title exclusions instead of retyping them: `[links.auto] exclude_titles = [...]`,
  `exclude_target_globs = [...]`, `first_only = true` in `.hyalo.toml`. Flags extend those lists
  rather than replacing them, and a run whose config exclusions removed candidates reports
  `config_excluded_titles` plus the `config_excluded_mentions` those titles accounted for.
  `--no-first-only` forces first-only OFF for one run when the config persists
  `first_only = true`. When a candidate title looks noisy — an ordinary English word,
  or unusually frequent (>= 25 matches and >= 2.5% of the run, which also catches non-English
  titles) — it is **held back** rather than proposed (DEC-286, iter-267), listed under
  `results.default_excluded_titles` with `default_excluded_mentions`, and named in a stderr
  `note:` with its count, share and the `--exclude-title` flag that reproduces the choice.
  Setting `[links.auto] exclude_titles` hands the decision to your own list and the built-in
  stop-list steps aside entirely; `--no-warn-common-titles` /
  `[links.auto] warn_common_titles = false` switches off both the exclusion and the note and
  proposes every candidate again.
  Inert everywhere, including across a wrapped line: frontmatter, code/HTML-comment spans,
  existing `[[wikilinks]]`/`[markdown](links)`, ANY well-formed `[...]` bracket span (not just
  real links — covers CommonMark reference links, and also undefined bracketed mentions like
  style-guide placeholders or PR area tags, since inserting `[[target]]` touching or inside an
  unrelated bracket produces nested bracket soup hyalo's own resolver misreads), bare URLs,
  Liquid/Jinja expressions, and raw HTML tags. When the matched text differs from
  the emitted target — including only by case — `--apply` writes `[[target|matched text]]`
  instead of silently rewriting the prose to the bare target.
- **Move/rename (single file)**: `hyalo mv old.md --to new.md` (rewrites links across the vault)
- **Move/rename (batch)**: `hyalo mv --glob 'iterations/*.md' --property status=completed --to iterations/done/` (dry-run by default; add `--apply` to commit; builds link graph once for all files; use `--on-conflict=skip` to skip collisions)
- **`mv` destinations resolve like sources** (DEC-304, iter-273): a CWD-relative destination
  carrying the configured vault dir works from the project root — with `dir = "kb"`,
  `hyalo mv kb/a.md kb/sub/a.md` lands at `kb/sub/a.md`, not `kb/kb/sub/a.md`. All four
  destination forms (positional `DEST`, `--to <file>`, `--to <dir>/`, batch) behave the same,
  and a `--to dir/` whose directory does not exist says so instead of suggesting `dir/.md`.
  `--on-conflict` takes `error` (default) or `skip` and nothing else — an unknown policy is a
  usage error (DEC-305) — and `skip` works in single-file mode too: the source stays put and is
  reported under `skipped` at exit 0. Batch `mv` runs the split-frontmatter-link sweep once for
  the whole batch and reports each move's own `moves[].frontmatter_links_skipped` (DEC-306).
- **Create new file from schema**: `hyalo new --type <name> --file <vault-relative-path>` (scaffold a skeleton; then run `hyalo lint --file <path>` to see what to fill in; add `--index` to patch an existing `.hyalo-index` in place so subsequent `--index` queries see the new file without a full rebuild). `--dry-run` prints the scaffold and writes nothing — not even the parent directory (DEC-285, iter-267). Placeholders are deliberately un-fillable: a required `string` gets `TBD`, a required `number`/`date`/`datetime`/`boolean` with no schema `default` is written **empty** (`rating:`), which lint reports as `required property must not be empty`; a schema `default` (including `$today`) is emitted verbatim. `new` takes no `--property`: it writes only what the schema declares, so chain `hyalo new --type <name> --file <path> && hyalo set <path> --property k=v` to set anything else
- **A named path is a promise** (DEC-301, iter-273): a `--file` or positional path whose YAML
  frontmatter will not parse fails the run (exit 1, the diagnostic in `cause`, a
  `lint --rule HYALO005` hint) instead of returning an empty result set; a `--files-from` list
  keeps batch semantics and counts it at exit 0. With `--index`, a named path the snapshot has
  never seen is read from disk for that run, so a note created since the last `create-index` is
  never invisible; a path in neither place is still `file not found`, exit 1. `--file`/`--glob`
  now keep `broken_anchor` and `suggested_fragment`, so the four ways of selecting one file
  return identical link JSON. And `lint --rule X` reports rule X only — a frontmatter parse
  error is HYALO005's finding and is otherwise a counted skip.
- **Lint markdown + frontmatter**: `hyalo lint`, `hyalo lint --strict` (promotes missing-type and undeclared-property warnings to errors), `hyalo lint --rule HYALO001 --detailed`, `hyalo lint --fix --dry-run`, `hyalo lint --fix`
- **Diff-aware lint (CI)**: `git diff --name-only origin/main...HEAD | hyalo lint --files-from -` — scope any command to a caller-supplied file list; non-.md paths and deleted files are silently skipped (counters in JSON envelope). Three-dot `origin/main...HEAD` (merge-base) keeps a stale branch scoped to files it changed. A path named explicitly — positionally, with `--file`, or through `--files-from` — is linted **even when `[lint] ignore` matches it** (DEC-284, iter-267), which is what a diff gate wants; `--glob` and the bare vault sweep still honour the ignore list, so select paths with `--glob` when you want it applied
- **Gate broken links (HYALO006)**: `hyalo lint --rule HYALO006` flags wikilinks/markdown links that point at a non-existent vault file (link TARGET only — broken `#heading` anchors are not checked here); `hyalo lint --strict` promotes it to an error so CI fails on a broken link. Resolution is vault-wide even under `--files-from`, so a diff-scoped file linking to an untouched-but-existing file is not a false positive.
- **Out-of-vault targets**: a link resolving above the scanned directory (`../../CONTRIBUTING.md`) is flagged `out_of_vault` rather than broken — `hyalo links` counts it under `out_of_vault`, `hyalo summary` under `links.out_of_vault`, and `find --broken-links` skips a file whose only unresolved link escapes the vault.
- **Detect broken heading anchors**: `hyalo find --broken-links` reports a `[[Foo#Section]]` / `[t](foo.md#Section)` whose target file exists but whose `#Section` heading does not, as a `broken_anchor` category distinct from a broken target (never both on one link). A fragment matches either the raw heading text (case-insensitive, Obsidian style) or the rendered GitHub slug — `#sub-section` matches `### Sub Section`, with `-1`/`-2` suffixes for repeated headings. Same-file fragments (`[b](#nope)`, `[[#nope]]`) are checked against the file's own headings and reported with `target: ""`; `^block-id` refs are skipped. Rebuild the index (`hyalo create-index`) after upgrading to pick up anchor data on the `--index` path. `find --fields links` (no `--broken-links` filter) always inventories same-file anchors, resolvable or not — `broken_anchor` is the verdict field, not a presence filter. A heading carrying a template expression (`## {% data variables.x %}`, `{{ y }}`, `${z}`) renders to an anchor hyalo cannot compute, so no anchor into that file is ever reported broken — same marker set `links fix` uses to leave templated destinations alone.
- **Link kinds (iter-261/262)**: every entry in `find --fields links` carries `kind` —
  `wikilink` | `embed` (`![[…]]`) | `markdown` | `frontmatter` (a `[[wikilink]]` in a YAML
  frontmatter value) | `external` (any `scheme:` URI: `https:`, `obsidian://`, `mailto:`,
  `file://`) | `attachment` (resolved to a non-`.md` vault file — an image, a PDF, an Obsidian
  `.base`). `external` and `attachment` links are never broken: they stay out of
  `find --broken-links`, `summary.links.broken` and HYALO006, and are not graph edges for
  `--orphan`/`--dead-end`. Text mode prints the kind after the arrow unless it is `wikilink`.
  `![alt](img.png)` is inventoried as an `embed` (DEC-297, iter-272) so a missing image
  surfaces, and a same-file anchor keeps its syntax: `[text](#frag)` is `kind: "markdown"` with
  its link text, `[[#H]]` stays `wikilink`.
- **Frontmatter wikilinks are graph edges** (DEC-269, iter-262): a `[[wikilink]]` in **any**
  frontmatter value — `categories: ["[[Books]]"]`, `type: "[[Author]]"`, a nested map, quoted
  or bare — counts for `backlinks`, `find --orphan`/`--dead-end`/`--broken-links`,
  `summary.links`, HYALO006 and the `--sort links_count|backlinks_count` keys, and `mv`
  rewrites it in place preserving the quoting. Each entry carries `kind: "frontmatter"`, the
  `property` it came from, and its frontmatter line. `[links] frontmatter = false` in
  `.hyalo.toml` narrows the scan back to `related`/`depends-on`/`supersedes`/`superseded-by`;
  `[links] frontmatter_properties = [...]` names your own list. `hyalo config` reports both
  under `links.frontmatter` / `links.frontmatter_properties`.
- **`set` on a list property** (DEC-270, iter-262): `set K=<scalar>` on a property that holds a
  list replaces it — `set` means replace — and says so on stderr, with the affected files under
  `list_collapsed` in JSON. Use `hyalo append` when the list should stay a list.
- **Frontmatter `aliases:` resolve wikilinks** (DEC-296, iter-272): `[[Leah]]` resolves to the
  note declaring `aliases: [Leah]` — a list or a bare string, matched case-folded. A filename or
  path match always wins, an alias claimed by two notes is ambiguous rather than resolved, and
  `[[alias#Heading]]` / `[[alias|label]]` work. `kind` stays `wikilink`; the entry carries
  `via: "alias"`. Alias links count for `backlinks`, `--orphan`/`--dead-end`,
  `summary.links` and HYALO006; `links fix` never proposes or fuzzy-matches a rewrite for one,
  and `mv` leaves them alone (the alias moves with the note). `[links] aliases = false` restores
  filename-only resolution; `hyalo config --jq '.results.links.aliases'` reports it.
- **Resolution folds case everywhere** (DEC-267): `[[AidenLx]]` resolves to `People/aidenlx.md`
  on every platform, not only on a case-insensitive filesystem. Opt out with
  `[links] case_insensitive = "false"`; `links fix --case-insensitive` now only suppresses the
  cosmetic `link-case-mismatch` rewrite plans.
- **Case plans never touch a `site_prefix` link, and a rewrite keeps its form** (DEC-295,
  iter-271): a site-absolute link carrying the configured `site_prefix`
  (`/en-US/docs/Web/CSS/Guides/Anchor_positioning`) is written in the *site's* URL convention,
  which the on-disk folder casing does not govern — no `link-case-mismatch` plan is produced for
  it. Broken site-absolute links are still fixed. For every strategy, the rewrite keeps the
  incoming form: a directory link stays a directory link (trailing slash included), an authored
  `.md` stays `.md`, and neither `/index` nor `.md` is appended to a form that lacked it.
- **`lint` respects code blocks and markdownlint-disable comments** (iter-271, DEC-294): a rule
  that lints prose does not fire inside a fenced (``` / `~~~`) or indented code block or an HTML
  comment. The exceptions are the rules whose subject IS the block — MD031/MD040/MD046/MD048
  (the fence), MD047 (the final newline) and MD010 (a hard tab in a sample is a real problem,
  as markdownlint also holds). MD031 additionally stays quiet at the opener of a fence that
  never closes. Silence a region the standard way:
  `<!-- markdownlint-disable no-hard-tabs -->` … `<!-- markdownlint-enable no-hard-tabs -->`,
  plus `-disable-line`, `-disable-next-line`, `-disable-file` and `-enable-file`, taking rule
  ids or aliases; with no ids a directive covers every rule, HYALO ones included.
- **Frontmatter closes only at column 0** (DEC-293, iter-271): an indented `  ---` inside a
  block scalar is content, not a delimiter — it used to truncate the block, dropping every key
  after it and letting the next `set` overwrite the body. A block that never closes at column 0
  is reported as unclosed (`HYALO005`). `set`/`append` also never emit a block scalar containing
  a `---`/`...` line: such a value is written double-quoted so it round-trips.
- **`mv` guards ambiguous frontmatter links** (iter-271): a bare `related: "[[a]]"` whose stem
  matches two files is skipped and listed in `skipped_ambiguous` with the `property` it came
  from, exactly as a body `[[a]]` is; `--allow-ambiguous` rewrites both.
- **Attachments resolve like Obsidian**: `![[img.png]]` matches a unique basename anywhere in
  the vault, `![[sub/img.png]]` also resolves against the source folder, and
  `[[Templates/Bases/Books.base]]` resolves by path. `links fix` never matches across an
  explicit extension (DEC-266), so a broken `Companies.base` is unfixable rather than rewritten
  into `Company Template.md`.
- **Anchor suggestions** (DEC-268): a broken `#fragment` that is the prefix of exactly one
  heading in the target file carries `suggested_fragment` with the full heading text —
  `[[decision-log#DEC-068]]` → `DEC-068: Snapshot index format`. Reported, never auto-applied;
  an ambiguous prefix suggests nothing.
- **Locate a broken link**: every entry in `find --fields links` carries `line`, the 1-based source line — the same one `lint` (HYALO006) and `backlinks` report — and links are listed in document order. Text output renders it as `line 12: "target" → "path"`. For a `file:line` list an editor can jump to: `hyalo find --broken-links --jq '.results[] as $f | $f.links[] | select(((.kind == "external" or .kind == "attachment") | not) and ((.path == null and (.out_of_vault | not)) or .broken_anchor)) | "\($f.file):\(.line) \(.target)"'` (the `out_of_vault` exclusion matters: an out-of-vault link also has `path: null` but is not itself broken, and can appear alongside a genuinely broken link in the same file's listing)
- **Gate broken anchors in CI**: HYALO006 does not check anchors (see above), so use `hyalo find --broken-links --strict` instead — exits 1 if any file has a broken target or broken anchor, 0 otherwise. `--strict` is a general `find` flag (works with any filter, e.g. `find --property status=draft --strict`), not anchor-specific. `hyalo links fix` also gets a one-line stderr-adjacent note ("N broken anchor(s) — see `find --broken-links`") when anchors are broken but targets are not, and `hyalo summary`'s `links.broken_anchors` figure is distinct from `links.broken` (link-count vs file-count units, so don't expect the raw numbers to match).
- **Manage lint rules**: `hyalo lint-rules list`, `hyalo lint-rules show <ID>`, `hyalo lint-rules set <ID> --enabled false`, `hyalo lint-rules set <ID> --severity warn`

- **The index says when it is stale** (DEC-302/DEC-303, iter-273): when the directory-mtime
  probe finds nothing, hyalo compares each indexed file's recorded mtime against disk and names
  the first drift in `index older than vault (<file> changed on disk …)` — this is what catches
  an in-place overwrite, which moves no directory's mtime. It costs one `stat` per indexed file
  (~0.03 s over MDN's 14,375) and only runs on a vault the cheap probe called clean; an edit
  inside the same whole second as the snapshot is still invisible. A snapshot also records how
  many files `[scan] exclude` dropped when it was built, so `summary --index` reports the same
  `excluded` figure as a disk scan (change the patterns and it is ignored — rebuild).

Fall back to Edit for body prose changes, Write for new files, and Read when
hyalo doesn't cover the operation (e.g., reading raw markdown for rewriting).

Output format auto-detects (text on terminals, json when piped); pass `--format text`
or `--format json` to override. Run `hyalo <command> --help` if unsure.
