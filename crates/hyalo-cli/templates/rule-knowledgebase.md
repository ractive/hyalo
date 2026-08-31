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
  mutating command with exit 1 (reads continue on defaults, with a `-q`-proof warning).
- **A project-local `dir` must stay at-or-below the config directory**: an absolute `dir` or one
  whose `..` components net above where `.hyalo.toml` lives refuses *every* command (reads
  included) with a `-q`-proof error naming the file and value — `hyalo config` still reports it
  (`dir_out_of_bounds`) rather than being refused. Pass `--dir` explicitly if that wider scope is
  genuinely intended; an in-bounds relative `dir`, including a bounded `sub/../kb` round-trip, is
  unaffected.
- **Hints marked `[writes]`** (`=>` prefix in text, `"writes": true` in JSON) modify the vault or
  `.hyalo.toml`; `->` hints are read-only and safe to run unattended.
- **Read frontmatter/metadata**: `hyalo find --file <path>`, `hyalo properties`, `hyalo tags`
- **`find` results are compact by default**: every item carries `file`, `modified`, `size`
  (bytes), `lines`, `title`, `properties` and `tags`. `sections`, `tasks`, `links`, `backlinks`
  and `properties-typed` come only from `--fields` (or `--fields all`) — or automatically from the
  filter that implies them (`--section`, `--task`, `--broken-links`, `--orphan`, `--dead-end`,
  `--sort links_count|backlinks_count`). `title` is promoted out of `properties`, so read it as
  `.results[].title`, not `.results[].properties.title`.
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
  titles) — a stderr `note:` names it with its match count and share, and suggests
  `--exclude-title` for every offender; act on it or silence it with
  `--no-warn-common-titles` / `[links.auto] warn_common_titles = false`.
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
- **Create new file from schema**: `hyalo new --type <name> --file <vault-relative-path>` (scaffold a skeleton with `TBD` placeholders; then run `hyalo lint --file <path>` to see what to fill in; add `--index` to patch an existing `.hyalo-index` in place so subsequent `--index` queries see the new file without a full rebuild). `new` takes no `--property`: it writes only what the schema declares, so chain `hyalo new --type <name> --file <path> && hyalo set <path> --property k=v` to set anything else
- **Lint markdown + frontmatter**: `hyalo lint`, `hyalo lint --strict` (promotes missing-type and undeclared-property warnings to errors), `hyalo lint --rule HYALO001 --detailed`, `hyalo lint --fix --dry-run`, `hyalo lint --fix`
- **Diff-aware lint (CI)**: `git diff --name-only origin/main...HEAD | hyalo lint --files-from -` — scope any command to a caller-supplied file list; non-.md paths and deleted files are silently skipped (counters in JSON envelope). Three-dot `origin/main...HEAD` (merge-base) keeps a stale branch scoped to files it changed
- **Gate broken links (HYALO006)**: `hyalo lint --rule HYALO006` flags wikilinks/markdown links that point at a non-existent vault file (link TARGET only — broken `#heading` anchors are not checked here); `hyalo lint --strict` promotes it to an error so CI fails on a broken link. Resolution is vault-wide even under `--files-from`, so a diff-scoped file linking to an untouched-but-existing file is not a false positive.
- **Out-of-vault targets**: a link resolving above the scanned directory (`../../CONTRIBUTING.md`) is flagged `out_of_vault` rather than broken — `hyalo links` counts it under `out_of_vault`, `hyalo summary` under `links.out_of_vault`, and `find --broken-links` skips a file whose only unresolved link escapes the vault.
- **Detect broken heading anchors**: `hyalo find --broken-links` reports a `[[Foo#Section]]` / `[t](foo.md#Section)` whose target file exists but whose `#Section` heading does not, as a `broken_anchor` category distinct from a broken target (never both on one link). A fragment matches either the raw heading text (case-insensitive, Obsidian style) or the rendered GitHub slug — `#sub-section` matches `### Sub Section`, with `-1`/`-2` suffixes for repeated headings. Same-file fragments (`[b](#nope)`, `[[#nope]]`) are checked against the file's own headings and reported with `target: ""`; `^block-id` refs are skipped. Rebuild the index (`hyalo create-index`) after upgrading to pick up anchor data on the `--index` path. `find --fields links` (no `--broken-links` filter) always inventories same-file anchors, resolvable or not — `broken_anchor` is the verdict field, not a presence filter. A heading carrying a template expression (`## {% data variables.x %}`, `{{ y }}`, `${z}`) renders to an anchor hyalo cannot compute, so no anchor into that file is ever reported broken — same marker set `links fix` uses to leave templated destinations alone.
- **Locate a broken link**: every entry in `find --fields links` carries `line`, the 1-based source line — the same one `lint` (HYALO006) and `backlinks` report — and links are listed in document order. Text output renders it as `line 12: "target" → "path"`. For a `file:line` list an editor can jump to: `hyalo find --broken-links --jq '.results[] as $f | $f.links[] | select((.path == null and (.out_of_vault | not)) or .broken_anchor) | "\($f.file):\(.line) \(.target)"'` (the `out_of_vault` exclusion matters: an out-of-vault link also has `path: null` but is not itself broken, and can appear alongside a genuinely broken link in the same file's listing)
- **Gate broken anchors in CI**: HYALO006 does not check anchors (see above), so use `hyalo find --broken-links --strict` instead — exits 1 if any file has a broken target or broken anchor, 0 otherwise. `--strict` is a general `find` flag (works with any filter, e.g. `find --property status=draft --strict`), not anchor-specific. `hyalo links fix` also gets a one-line stderr-adjacent note ("N broken anchor(s) — see `find --broken-links`") when anchors are broken but targets are not, and `hyalo summary`'s `links.broken_anchors` figure is distinct from `links.broken` (link-count vs file-count units, so don't expect the raw numbers to match).
- **Manage lint rules**: `hyalo lint-rules list`, `hyalo lint-rules show <ID>`, `hyalo lint-rules set <ID> --enabled false`, `hyalo lint-rules set <ID> --severity warn`

Fall back to Edit for body prose changes, Write for new files, and Read when
hyalo doesn't cover the operation (e.g., reading raw markdown for rewriting).

Output format auto-detects (text on terminals, json when piped); pass `--format text`
or `--format json` to override. Run `hyalo <command> --help` if unsure.
