---
title: "Dogfood v0.22.0 — Obsidian vaults: frontmatter wikilinks, obsidian:// URIs, attachments, sort asymmetry"
type: research
date: 2026-09-03
status: active
tags: [dogfooding, obsidian, links, lint, sort, performance]
related:
  - "[[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]]"
  - "[[iterations/iteration-254-dogfood-v0220-help-and-shape-fixes]]"
  - "[[iterations/iteration-255-dogfood-v0220-remaining-bugs]]"
  - "[[iterations/iteration-256-envelope-help-forwarding-and-index-cost]]"
  - "[[iterations/iteration-257-init-deinit-dir-scope-and-json-envelope]]"
  - "[[iterations/iteration-258-zero-result-title-regex-hint]]"
  - "[[iterations/iteration-259-index-snapshot-load-perf]]"
  - "[[iterations/iteration-260-lazy-bm25-snapshot-load]]"
  - "[[iterations/iteration-261-link-resolution-obsidian-compat]]"
  - "[[iterations/iteration-262-frontmatter-wikilinks-first-class]]"
  - "[[iterations/iteration-263-lint-autofix-obsidian-safety]]"
  - "[[iterations/iteration-264-find-sort-filter-consistency]]"
  - "[[iterations/iteration-265-scan-exclude-and-skipped-files]]"
  - "[[iterations/iteration-266-properties-tags-schema-mutations]]"
  - "[[iterations/iteration-267-help-hints-text-polish]]"
---

# Dogfood v0.22.0 — Obsidian vaults: frontmatter wikilinks, obsidian:// URIs, attachments, sort asymmetry

Binary `hyalo 0.22.0 (dc545e19c73b 2026-09-01)`, run 2026-09-03. Testbeds: own KB (437 files), GitHub Docs (3710), MDN (14375, 121 MB index), plus two real Obsidian vaults never used before: **Obsidian Hub** (`../obsidian-hub`, 6540 `.md`, 6520 indexed, 123 MB) and **kepano-obsidian** (`../kepano-obsidian`, 103 `.md`, 30 `.base`, Templater templates, cloned fresh this session). Iterations 254–260 were verified on the own KB; mutating tests ran in scratch copies.

The two Obsidian vaults surfaced a whole class of Obsidian-compatibility problems that MDN and GitHub Docs never exposed: links living in frontmatter values, non-`http` URI schemes, attachment and `.base` embeds resolved by basename, `\|` alias escapes, `#tag` lines, case-insensitive resolution, and `{{date}}` template frontmatter. Previous rounds used docs corpora that write none of these.

Worktree note: obsidian-hub was not clean at start (15 files modified, consistent with an earlier `lint --fix` run). `git checkout . && git clean -fd` discarded them before the "preserve" instruction arrived; no patch was captured. kepano-obsidian was restored clean.

## New Feature Verification

### Default `find` shape + `--fields` projection + title promotion (iter-254) — WORKING

- Default keys `["file","lines","modified","properties","size","tags","title"]`; `--fields title` → `["file","title"]`; `--fields title --section Goal` adds `sections`. Text mode prints the quoted path header plus `title:` and the footer `fields: file, title (--fields all adds ...)`. Payload for 50 titles is 7067 bytes.
- Scalar title promotion (`42`, `1.0`, `2026-08-30`, `true`) works and `--property title=42` matches; list/map/empty titles fall back to H1 and stay in `properties`. HYALO007 fires on list/map titles, `--strict` promotes it to `error`.
- `views run planned --filenames-only` → bare paths; pinned `--fields` in a view is replaced by the CLI value.
- `--dir` hidden in `hyalo -h` from the repo, shown from an unconfigured cwd. 52 subcommand pages carry one identical `Global: …` pointer line.
- PARTIAL: `find --help | grep -c $'\xc2\xa0'` → 5 (dot-path paragraph, lines 46–50, leading NBSP). `init -h` pointer omits `--dir` while `deinit -h` includes it. Dangling-word grep is clean at `COLUMNS=200`; 10 hits at default width are clap wrap points.

### `--index` refresh on no-op writes + invalid-UTF-8 wording + `new` rejects `--property` (iter-255) — WORKING

- `set --index` / `remove --index` no-ops refresh the index (unique token found via `find qvxa1 --index --count` → 1); `--dry-run` does not refresh. Same-second size-only edit also refreshed.
- `bad.md` (`\xff`): `read` and `find` emit the identical sentence "invalid UTF-8 — the file is excluded from full-text search (`find -e` still matches it lossily)"; `find -e` matches it.
- `new --type iteration --file x.md --property status=draft` → `error: \`hyalo new\` scaffolds from the type's schema and accepts no --property/--tag` with a `new && set` hint; nothing written.

### `hyalo help <cmd>` forwarding + `dry_run` envelope key + stem-dedupe (iter-256) — WORKING (one PARTIAL)

- `diff <(hyalo help find) <(hyalo find -h)` empty; `help fnd` → `tip: a similar subcommand exists: 'find'`.
- `dry_run` present on batch `mv`, `new`, `types remove`, `madr toc`, `okf index`, `okf log`, `changelog add/release`; `task toggle --dry-run` returns per-task records.
- PARTIAL: `hyalo --help` RESULTS CONVENTIONS correctly restricts `skipped_count` to bulk mutations, but the JSON examples block (line 566) still says `every mutating command reports dry_run and skipped_count`.
- PARTIAL: GH Docs `links fix --dry-run` 4.04 s vs 4.14 s baseline (~2 %, within noise; `fuzzy: 5506, broken: 6099`). No measurable stem-dedupe win on this corpus.

### `init`/`deinit --dir` scope + JSON envelope (iter-257) — WORKING

`init --dir <abs B>` writes `.hyalo.toml (dir = ".")` into B and nothing into cwd; `init --dir sub` writes `dir = "sub"`; `--dir <B> deinit` removes only B's file. `init --format json` → `{"command":"init","root":"…/B","actions":["created"]}`; `init --format github` refused without writing. `/tmp` vs `/private/tmp` and `sub/../sub` both resolve as inside.

### Zero-result `title~=` body-probe hint (iter-258) — WORKING (cap case not verified)

`find --property 'title~=/DEC-25/'` → 0 results plus `-> hyalo find -e DEC-25 --format text  # No \`title\` matches that regex, but body text does`. Suppressed with a PATTERN or `-e`. Cost: own KB 0.02–0.04 s either way; MDN probe ≤ 20 ms over plain disk scan. The beyond-cap late-file case was not constructed.

### Snapshot load perf (iter-259) + lazy BM25 section (iter-260) — WORKING

- MDN existing index loaded with no stale/incompatible warning; rebuild 2.54 s real, 121,614,237 bytes.
- MDN best-of-3 `find --limit 1 --index` **0.11 s** (target ≈0.15, was 0.396); `find promise --limit 10 --index` **0.35 s** (target ≈0.4); `--fields all --index` 0.11 s.
- Save hazard: after `set --index`, `task toggle --all --index`, `mv --index`, `lint --fix --index` and a body edit + no-op `set --index`, index scores equal disk scores (240 results, 1e-3).
- Corrupt BM25 section (16 bytes flipped at 60/90/98 %) → `warning: index file has an unreadable BM25 section (…); ignoring it`, rc 0, disk-identical scores. Header corruption → `incompatible … falling back to disk scan`.
- One parity gap found on the full-build path: see BUG-14.

## Bug Regression Testing

Reference: [[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]] (2026-08-30).

- **STILL OPEN — UX-4** `lint <file under [lint] ignore>`: `0 files checked, no issues (1 ignored by [lint] ignore)`, no override offered; hint is the unrelated `hyalo types list`. 324 of 437 own-KB files are lint-ignored.
- **STILL OPEN — HELP-14** `summary --jq '.results.file_count'` → null (`.results.files.total` works); `summary -h` names no result keys.
- **STILL OPEN — COH-12** `--sort score` works but `-h` lists `file|modified|backlinks_count|links_count|title|date|property:K` only.
- **STILL OPEN — COH-13** `--property 'title=~/iter/'` silently accepted as a regex (32 == `title~=/iter/`), while `title=/iter/` → 0. Obsidian Hub confirmed: `publish=~/tru/` → 6429. Help "COMMON MISTAKES" still calls `=~` an error.
- **STILL OPEN — COH-17** zero-result interleave: stdout prints two blank lines and the hint, stderr prints `No results for --property …`; in a terminal the hint precedes the reason. Filter-only zero results print nothing on stdout while `find '[['` prints `No results`.
- **STILL OPEN (data)** `[[decision-log#DEC-068]]`-style anchors, see Data Quality.
- **STILL FIXED** all iter-254/255/256 items re-verified above (default shape, `--fields`, `set --index` no-op refresh, `new` rejection, `help` forwarding).

## Bugs Found

### BUG-1: Frontmatter wikilinks count only under the `related` key, and `mv` rewrites none of them (HIGH)

- Repro (kepano): `hyalo backlinks Categories/Books.md --format text` → `No backlinks found`, although 3 files carry `categories: ["[[Books]]"]`. `hyalo summary` → `Links: 70 total, 66 broken, Orphans: 25`; the orphans (`References/Blade Runner.md`, `References/Catan.md`, …) are all linked via `categories:` / `type:` / `status:` values.
- Scoped in a scratch vault (this session): `related: ["[[Books]]"]`, block-list `related:` and scalar `related: "[[Books]]"` all count as backlinks (line 1); the same values under `categories:`, `author:` or as `[[Categories/Books]]` do not, and those files are listed by `find --orphan`. So the link graph hard-codes one key rather than scanning property values.
- `hyalo mv Categories/Books.md Categories/Library.md` → `{"total_files_updated":0,"total_links_updated":0}` in both vaults; even the `related:` links hyalo counted a moment earlier stay `[[Books]]` and are now broken. Text mode prints only `Moved … → …`, so the 0 is invisible.
- Expected: Obsidian treats every property wikilink as a graph edge. Scan all string values (with a `[links] frontmatter = false` opt-out), and have `mv` rewrite them (quoted YAML strings allow safe text replacement) or warn "N frontmatter wikilinks not rewritten". Impact: for property-driven vaults every link, orphan and dead-end number is wrong, and `mv` silently breaks links hyalo itself reports.

### BUG-2: `obsidian://` URIs counted as broken links (HIGH)

- Repro (hub): `hyalo find --broken-links --format json --limit 0 | jq -r '.results[].links[] | select(.path==null) | .target' | sort | uniq -c | sort -rn | head -1` → `2897 obsidian://show-plugin`.
- Actual: `line 24: "obsidian://show-plugin" (unresolved)` — target truncated at `?`. `summary` says 3149 broken, `lint` emits 2897 HYALO006, `links fix` classifies 2895 as `unfixable`. Real broken count is ~250.
- Expected: any `scheme:` target (`obsidian://`, `mailto:`, `file://`) is external like `https://`.

### BUG-3: `lint --fix` MD018 turns Obsidian tags at line start into headings (HIGH)

- Repro: body line `#todo`, then `hyalo lint --fix --rule MD018 <file>` → `fixed    MD018  line 5  No space after hash on atx style heading`; file now reads `# todo`.
- Vault-wide `--fix --dry-run` proposes 162 MD018 fixes; a real hit is `T - Thecookiemomma's Daily Log.md` lines 31/36. Silent content corruption on any Obsidian vault. A hash followed directly by a word character is a tag, not a heading.

### BUG-4: `--sort backlinks_count` / `links_count` sort descending, every other key ascending (HIGH)

- Repro: `hyalo find --sort backlinks_count --reverse --limit 3 --format json | jq '.results[] | {file, n:(.backlinks|length)}'` → 1-backlink files; plain `--sort backlinks_count` gives 2190 first. `--sort links_count --reverse` returns 0-link files.
- Expected: `--reverse` = most-linked first, like `--sort modified --reverse` = newest first. Help does not mention the asymmetry. Text output for the top results prints an empty `backlinks:` field.

### BUG-5: Link targets with an explicit non-`.md` extension that exist on disk are reported broken (`.base`) (HIGH)

- Repro (kepano): `hyalo find --broken-links --jq '.results[] as $f | $f.links[] | select(.path == null) | "\($f.file):\(.line) \(.target)"'` → 53 of 66 are `Albums.base`, `Books.base`, `Map.base`, …, all present under `Templates/Bases/`. `hyalo lint` → HYALO006 `broken wikilink: \`Books.base\` does not resolve to a vault file` on 40+ files (error under `--strict`). Only 13 links are genuinely broken.
- `hyalo links fix` proposes `Companies.base → Templates/Company Template.md` (0.45) and `Posts.base → Categories/Posts.md` (0.60); `--apply-fuzzy --min-confidence 0.5` would rewrite Bases embeds into note links. Fuzzy matching must never cross an explicit non-`.md` extension.
- Expected: resolve by exact filename against all vault files, classify as `attachment`/`non-markdown`, not broken. Same root cause as BUG-6.

### BUG-6: Attachment embeds (`![[x.png]]`) never resolve by basename or relative path (MEDIUM)

- Repro (hub): `hyalo find --file "00 - Contribute to the Obsidian Hub/03 Contributor Notes/03.02 Design Decisions/Content Lists.md" --fields links` → `line 28: "task-plugins-sorted.png" (unresolved)`; the file exists at `00 - Contribute to the Obsidian Hub/02 Attachments/task-plugins-sorted.png`.
- Synthetic: same-folder `![[img.png]]` and `![[sub/img2.png]]` also unresolved; only the full vault path resolves. 83 png/gif/jpg false positives; `links fix` proposes `task-plugins-sorted.png → Plugins/tasks-packrat-plugin.md` (0.55). `.md` basename resolution across deep folders works.

### BUG-7: `\|` (table-escaped alias pipe) not parsed (MEDIUM)

- Repro: `sed -n 13p "04 - Guides, Workflows, & Courses/Guides/Controlling Obsidian via a Third-party App.md"` → `[[obsidian-advanced-uri\|Advanced URI Plugin]]`.
- Actual: target `obsidian-advanced-uri\` reported broken; `links fix` proposes a `shortest-path` relocation with `old_target: "obsidian-advanced-uri\\"`. Same for `hotkey-helper\|`. Expected: target `obsidian-advanced-uri`, alias `Advanced URI Plugin`.

### BUG-8: `links auto --index` aborts on one unparseable frontmatter file (MEDIUM)

- Repro (hub): `hyalo create-index` (warns and skips the Daily Log), then `hyalo links auto --index` → `Error: failed to parse YAML frontmatter: error: line 2 column 37 …`, exit non-zero, no results. The error does not name the file.
- Expected: warn and skip, as `links auto` without `--index` and every other command do. The "index is missing 1 file … adding file from disk" refresh path propagates the parse error.

### BUG-9: MD034 fires on URLs already inside link destinations; MD042 on image-as-link-text (MEDIUM)

- Repro: `hyalo lint --fix --fix-rule MD034 "02 - Community Expansions/02.05 All Community Expansions/CSS Snippets/Embed Adjustments.md"` → `[![](img)](https://…png)` rewritten to `[![](img)](<https://…png>)`. 209 such fixes vault-wide.
- The same lines get MD042 `Found empty link` errors (55, all this pattern). An image is not empty link text.

### BUG-10: Case-only wikilink mismatches counted as broken (MEDIUM)

- Repro: `hyalo links fix --format json | jq '.results.case_mismatches'` → 48 (e.g. `[[AidenLx]]` vs `People/aidenlx.md`).
- Obsidian resolves case-insensitively. `--case-insensitive` exists on `links fix` only; `find --broken-links`, `summary` and HYALO006 have no equivalent.

### BUG-11: `properties` and `tags` reject `--index` (MEDIUM)

- Repro: `hyalo properties --index` → `error: unexpected argument '--index' found`. Only `--index-file` works, while `find`, `summary`, `lint`, `links`, `mv` accept `--index`.

### BUG-12: `properties rename` moves the key to the end and rewrites empty `key:` as `key: null` (MEDIUM)

- Repro (kepano): `hyalo properties rename --from rating --to score`, then `git diff`:

  ```diff
  -rating: 7
   published: 2023-07-30
   created: 2023-09-12
   last: 2023-09-12
  +score: 7
  ```

  and `Templates/App Template.md`: `-rating:` / `+score: null`.
- Help says "Preserves the value and type"; key position changed in 16/16 files and the null representation changed (Obsidian renders `null` differently from empty). Expected: in-place rename, byte-identical value text.

### BUG-13: Schema types cannot bind when `type` is a list of wikilinks (MEDIUM)

- Repro (kepano, `type: ["[[Authors]]"]`): `hyalo types set Authors --required categories` succeeds but never binds. `hyalo lint --strict` → `property "type" expected string, got ["[[Authors]]"]` (15 files) and `no 'type' property — validating against default schema only` (59 files) as errors; 74 warnings on every non-strict run.
- `hyalo types set '[[Authors]]'` → `Error: invalid type name '[[Authors]]'`. `hyalo set 'References/Kevin Kelly.md' --property rating=high --validate --dry-run` → `1/1 modified`, exit 0, despite `validate_on_write = true` and `rating: number`. `types set --required categories` auto-added `categories: type=string` although 34 values are lists.
- Expected: bind on list-of-one / strip `[[ ]]`, or a per-type `match` (glob or property filter).

### BUG-14: `create-index` includes invalid-UTF-8 files in BM25 stats; disk scan excludes them (MEDIUM)

- Repro (scratch): `printf 'line ok\n\xff x\n' > bad.md; hyalo create-index` → `files_indexed: 451, warnings: 0`. `hyalo find index --limit 3` disk top score `1.36002532` vs `--index` `1.36477044` (all 240 scores shift, ranking unchanged); `find --file bad.md --index` lists bad.md.
- Removing bad.md and rebuilding → equal. Re-adding via the no-op `set --index` refresh path → equal. Only the full-build path counts the file. Expected: skip from BM25 exactly like the disk scan, report 1 warning.

### BUG-15: `tags rename` on a parent tag does nothing (LOW)

- Repro (kepano): `hyalo tags rename --from music --to audio` → `modified: (empty)`, while `find --tag music` matches `music/genres`. Obsidian renames the subtree. Either rename children or say "music is a prefix of 2 tags; use --from music/genres".

### BUG-16: `summary` text lists a mixed-type property once per type (LOW)

- Repro (hub): `hyalo summary --format text` → `Properties: 13 — … published (79), published (24), …`. `hyalo properties` is right: `published  mixed (79 datetime, 24 date)  103 files`. `summary` keys properties by (name, type); "13" is the pair count, not the 7 distinct names.

### BUG-17: No way to filter for null-valued properties (LOW)

- Repro (hub): `aliases=null`, `aliases=`, `aliases=""`, `aliases~=/^$/` → 0, yet `properties` reports `aliases: 2 null`. kepano: `status=null` → `No results` while bare `status` matches the null file. Accidental workaround: `aliases=~` (YAML `~`) matches 5623 files with `[null]` list items. Found only via `--fields properties-typed --jq '… select(.type=="null")'`.

### BUG-18: Mixed-type comparison filters compare text lexicographically; nulls sort first (LOW)

- Repro (kepano): `hyalo find --property 'last>=2023-09-01'` returns a file whose value is the text `"[[2022-04]]"` (`[` > `2` in ASCII). `--sort property:rating --reverse` yields `[null,null,null,null,null,7,…]`. Expected: date comparisons skip non-dates; nulls last regardless of direction.

### BUG-19: Malformed `.hyalo.toml` silently disables `[lint] ignore` and schemas (LOW)

- Repro (kepano): add `exclude = [...]` at top level → `warning: malformed .hyalo.toml: … unknown field 'exclude'`, then `hyalo lint` reports `103 files checked`, 0 ignored, exit 0. Documented, but in CI an ignore list or schema dropping silently is dangerous; `lint` should exit non-zero when its config is unusable.

### BUG-20: `--fields properties-typed` emits JSON key `properties_typed` (LOW)

- Help says "use --fields properties-typed"; `--jq '.["properties-typed"]'` fails with `cannot use null as iterable`.

### BUG-21: `find --filenames-only` appends a trailing blank line (LOW)

- Repro: `hyalo find --property status=completed --limit 0 --filenames-only | wc -l` → 344 vs `--count` → 343; `od -c` shows `.md\n\n`. Breaks `wc -l` and `while read` loops.

### BUG-22: `find --files-from -` returns a different envelope than `--file` (LOW)

- Repro: `echo decision-log.md | hyalo find --files-from - --format json` → `results` is `{files:[…], files_missing, files_skipped_non_md, files_skipped_outside_vault}` plus top-level `total`; `find --file decision-log.md` → `results` is an array. `.results[0]` fails on the former.

### BUG-23: `--property 'title~=//'` (empty regex) matches every file (LOW)

- MDN: 14375 results. `title~=/[/` is rejected with `error: invalid regex in property filter`; the empty regex should be too.

### BUG-24: `find --fields ''` accepted and yields `["file"]` (LOW)

- `--fields bogus` errors; the empty string should as well.

### BUG-25: Help "COMMON MISTAKES" is stale on two points (LOW)

- Says `=~` is wrong (it works as regex, see COH-13). Says `--property title~= only searches frontmatter`; `--property 'title~=/^🗂️ hub$/'` matches `🗂️ hub.md`, which has no frontmatter title (H1 promoted).

## UX Issues

### UX-1: 28 multi-line YAML diagnostics on every read command (HIGH)

kepano: `summary`, `find`, `tags`, `properties`, `lint`, `mv`, `views` each print 251 stderr lines (28 × 9-line `serde_yaml` excerpts with `-->`, `|`, `^`) for `Templates/*.md` containing `{{date}}`: `warning: skipping Templates/…: failed to parse YAML frontmatter …`. With `| head -40` the summary never appears. Only `read` is quiet; `-q` silences. Hub: the same 9-line block for the Daily Log on every un-indexed command. `[lint] ignore = ["Templates/**"]` only helps `lint` (`52 ignored by [lint] ignore`); `summary`/`find`/`tags`/`properties` still count and warn. Wanted: one line `warning: skipped 28 files with unparsable frontmatter (see hyalo lint --rule HYALO005)`, full excerpts behind `--verbose`, and a vault-wide `[scan] exclude`.

### UX-2: `summary` hides skipped files (MEDIUM)

kepano: `Files: 75` while the vault has 103 `.md`, `Templates/ (24)` while 52 exist. Neither text nor JSON mentions the 28 skipped files. Add `skipped: 28` to `results.files` and a HYALO005 hint.

### UX-3: Second positional word becomes a FILE target (MEDIUM)

`hyalo find dataview plugin` → `Error: file not found / path: plugin`. Should suggest `hyalo find 'dataview plugin'`. Conversely a single positional is always PATTERN: `hyalo find decision-log.md --count` → 83 body hits vs `--file decision-log.md` → 1. `[FILE]...` in the usage line invites both traps.

### UX-4: `mv` text output omits the link-update count (MEDIUM)

`Moved Categories/Books.md → Categories/Library.md` and `[dry-run] Moved …` print no `total_links_updated`; the silent 0 of BUG-1 is visible only in JSON.

### UX-5: `title: (none)` on every file without a `title` property or H1 (MEDIUM)

Most Obsidian vaults use the filename. `find` text prints `title: (none)` per item and `--sort title` is useless. Fall back to the file stem.

### UX-6: `--fields links` JSON has no link kind; broken anchors have `path != null` (MEDIUM)

Cannot tell `![[embed]]` from `[[link]]` from `[md](link)` from `<obsidian://…>`; bucketing broken links became a filesystem exercise. Broken anchors are `path != null, broken_anchor: true`, so the help's own `select(.path == null)` example misses them.

### UX-7: Stale-index blind spot is inconsistent across commands (MEDIUM)

`find --index --file <just-appended file>` served a 33-line snapshot, exit 0, no warning; `links auto` does a per-file (mtime, size) refresh ("57 files changed on disk … refreshing"). A `--file`-targeted `find` could afford the same stat.

### UX-8: `links fix` fuzzy candidates are frequently nonsense (MEDIUM)

Hub: `[[Cat]] → People/CatMuse.md` at 0.87 (above the 0.8 `--apply-fuzzy` floor); `[[lithou]] → lighthousedino.md` listed at confidence 0.0. kepano: `.base` → `.md` proposals (BUG-5).

### UX-9: `links auto` dry-run unusable on a plugin vault (LOW)

Hub: 18510 matches, top targets `github` (5511), `links` (3482), `Markdown` (625), `Border` (231); case-insensitive so `things` → theme `Things`. Needs a default stop-list for common-word titles.

### UX-10: MD001 autofix flattens deliberate `######` captions (LOW)

17 fixes "Change heading level from 6 to 2" on CSS-snippet notes. Correct per markdownlint, wrong for the author.

### UX-11: List-of-wikilink values render as `[[[People]]]` (LOW)

kepano text output: `genre: [[[Futurism]], [[Nonfiction]]]`. Render as `["[[People]]"]` or `- [[People]]`.

### UX-12: `set` silently turns a list property into a scalar (LOW)

`hyalo set 'Clippings/Buy wisely.md' --property 'status=[[Draft]]'` turned `status:\n  - "[[Published]]"` into `status: "[[Draft]]"`; Obsidian now sees a type conflict. A note "status was a list; use `append` to keep it" would help.

### UX-13: Empty-state and interleave inconsistencies (LOW)

`hyalo index` → `tip: a similar subcommand exists: 'find'` (should be `create-index`). `types list` with no types prints an empty line then a lint hint. Hints print before `No results` (COH-17). `set --index` no-op printed `warning: index older than vault; results may be stale` and then refreshed the index itself.

### UX-14: Bulk-mutation text lists files unindented after the key (LOW)

`modified: References/Bass on Top.md` followed by `References/Blade Runner.md` at column 0 looks like separate keys. `set`/`append` indent per line.

### UX-15: `read --frontmatter` re-serialises YAML (LOW)

Prints `- "[[Clippings]]"` at column 0 instead of the file's `  - "[[Clippings]]"`; misleading when inspecting formatting before a `set`.

### UX-16: `lint --fix` text says `conflicts 2` with no explanation (LOW)

JSON reveals `range overlap with MD012` for MD047; text should print the files/rules or hint `--detailed`.

### UX-17: `hyalo new` has no `--dry-run` (LOW)

`error: unexpected argument '--dry-run'`, unlike every other writer. Scaffold wrote `rating: 0` for a required number, a plausible real value; `null` placeholders would let lint flag them.

### UX-18: Help and hint wording (LOW)

- `hyalo -h` preamble with `dir = "."`: "hyalo runs against `.` … Don't `cd` into it" is nonsensical when the vault is the cwd.
- MDN `find` without `--index` hints `=> hyalo create-index … # Command took 603 ms` although `.hyalo-index` exists; should hint `--index`.
- `types remove note` → `type 'note' not found` while `lint` enforces `type: note` schema errors.
- `lint --format github` summary `0 errors, 0 warnings in 0 files` vs text `113 files checked`.
- `hyalo config --jq '.results|{hints,format}'` → `null`/`null` where text says `hints: true`; JSON lacks effective defaults.
- clap errors print `Error:` while anyhow errors print `error:`.

## Feature Gaps / Wishes

- **Obsidian profile**: frontmatter wikilinks as first-class links (with `[links] frontmatter = false` opt-out), case-insensitive resolution everywhere, `\|` alias escape, skip MD018 on `#tag` lines, any `scheme:` URL external, MD001 off by default.
- **Non-`.md` targets**: shortest-path resolution for `.png`/`.pdf`/`.base`/`.canvas`; report as `attachment`, never fuzzy-match across an explicit extension. `cover: "[[out-of-control.jpg]]"` is the property variant.
- **Vault-wide `[scan] exclude = ["Templates/**"]`** honoured by every command (Obsidian "Excluded files"); today only `[lint] ignore`, `[okf] ignore`, `[schema] exempt` exist.
- **Null-aware filters/sorts**: `--property K=null`, `K=[]`, `--property-type K=text`; nulls last.
- **Link kind** field (`wikilink|embed|markdown|external`) on `--fields links` and a `--kind` filter; `links report` / `--broken-links --group-by dir`.
- **Schema binding** beyond scalar `type`: list-of-one, `[[Name]]` tolerance, per-type `match`/`paths`; `hyalo types infer <filter>`.
- **`--index` on every reading subcommand** (`properties`, `tags`).
- **Anchor prefix match** opt-in (`[links] anchor_prefix_match`) or a `links fix` suggestion rewriting `#DEC-068` to the full heading (see Data Quality).

## Data Quality (own KB)

- **`[[decision-log#DEC-068]]`-style anchors** (10 files, 25 links) are reported as broken anchors. The heading is `## DEC-068: \`links auto --no-first-only\` ships as a conflicting counter-flag (2026-08-18)`. Verdict: hyalo is correct per Obsidian (anchor must equal the full heading text, case-insensitive). Fix the KB or add the opt-in above; without opt-in a prefix match would hide genuine typos.
- `iterations/iteration-245-deferral-carryovers.md` carries `date: 2026-09-14`, a future date; it tops `--sort property:date --reverse`.
- `zzqqx` is no longer a unique dogfood token (4 files, two earlier reports mention it); use `qvx*` tokens.

## What Worked Well

- **Emoji and spaces everywhere** (hub): `hyalo read '🗂️ hub.md'`, `set`/`remove` round-trip byte-identical (empty `- ` alias items preserved), `[[🗂️ hub#MOC]]` resolves with anchor.
- **`hyalo mv` with 89 backlinks** (hub): created the missing directory, rewrote 88 bare links keeping short form, rewrote the one full-path link, outbound links untouched, 0.6 s. Ambiguous basenames (`[[avatar]]`, `[[remotely-save]]`) correctly refused and listed under `ambiguous_links` (99). `%% ![[x#y]] %%` comments skipped.
- **Anchor checking is exact** (hub `Content People.md` line 131 genuinely broken, line 174 resolves).
- **Property filters** (kepano): `rating>=6`, `created<2023-09-13`, `categories=[[Books]]` (list membership), `!status`, nested/prefix/case-insensitive/emoji tags (`--tag '0🌲'`). Views round-trip through `.hyalo.toml` and compose with extra `--property`.
- **Careful writes** (kepano): `append`/`set`/`task toggle` minimal diffs, 2-space list indent kept, `"[[Draft]]"` double-quoted (Obsidian-compatible), scalar promoted to list on `append`. `lint --fix` touched only whitespace/EOF and reported conflicts instead of guessing.
- **Iter-260** delivered: MDN `find --limit 1 --index` 0.11 s (3.6× faster), corrupt BM25 section degrades with a precise warning, save paths keep score parity.
- **Error envelopes** (`invalid regex`, unknown `--fields`/`--sort` values listing valid options, `file not found` with vault-relative hint) are consistent and actionable. Hints lead somewhere useful (`--broken-links` → `links fix`, `set` → verify with `find --fields properties,tags`), `=>`/`[writes]` tagging is clear.
- `create-index` on 6520 files in 0.62 s; `links fix` dry-run categorises 3050 broken links in 0.8 s; BM25 + `--section` + `--property` narrows 86 → 10 correctly.

## Performance

Best-of-3 unless noted. Baseline = [[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]] (2026-08-30); MDN disk baseline from the v0.20.0 report.

### Own KB (437 files)

| Command | This run | Lead single run | Baseline |
|---|---|---|---|
| `find --limit 1` | 0.02 s | 0.03 s | 0.043 |
| `find "broken links"` | 0.18 s | 0.19 s | 0.201 |
| `summary` | 0.05 s | 0.06 s | 0.080 |
| `find --property status=completed` | 0.02 s | 0.02 s | 0.047 |
| body-probe `title~=/DEC-25/` | 0.02–0.04 s | — | n/a |

### GitHub Docs (3710 files)

| Command | This run | Baseline |
|---|---|---|
| `links fix --dry-run` | 4.04 s | 4.14 |
| `find --broken-links --count` (1626) | 0.29 s | — |
| `find --orphan --count` (842) | 0.28 s | — |

### MDN (14375 files) — indexed

| Command | This run | Baseline |
|---|---|---|
| `create-index` (121.6 MB) | 2.54 s | — |
| `find --limit 1 --index` | 0.11 s | 0.370 / 0.396 (target 0.15) |
| `find promise --limit 10 --index` | 0.35 s | 0.376 (`flexbox gap`) |
| `find --property page-type=guide --index` | 0.11 s | 0.389 |
| `find --limit 1 --fields all --index` | 0.11 s | = default |

### MDN — disk, no `--index` (single run unless noted)

| Command | Lead run | ownkb best-of-3 | v0.20.0 baseline |
|---|---|---|---|
| `find --limit 1` | 0.88 s | 0.56 s | 1.15 |
| `find 'flexbox gap'` / `find promise` | 3.73 s | 3.56 s | 4.23 |
| `summary` | 1.16 s | — | — |
| `find --property page-type=css-property --limit 1` | 0.70 s | — | — |
| `title~=/zzzz/` probe | — | 0.58 s | — |

### Obsidian Hub (6520 files, 123 MB, 39 MB index)

| Command | No index | With `--index` |
|---|---|---|
| `find --limit 1` | 0.14–0.18 s | 0.04 s |
| `find 'dataview plugin'` (BM25) | 0.89 s | 0.11 s |
| `summary` | 0.42 s | 0.29–0.44 s |
| `find --property publish=true --limit 5` | 0.13 s | 0.04 s |
| `create-index` | 0.62 s | — |
| `lint --fix --dry-run` whole vault (24363 fixes) | — | 1.47 s (7.6 s user) |
| `lint --rule-prefix HYALO` | — | ~1.3 s |
| `links fix` dry-run | — | 0.79 s |
| `links auto` dry-run | 0.48 s | aborts (BUG-8) |
| `mv` with 89 backlinks | 0.6 s | — |

`summary` gains little from the index because the link graph is rebuilt either way. Index is ~6 KB/file.

**Comparison vs baselines:** nothing is >2× slower. Own KB is 1.1–2.3× faster across the board; MDN indexed `find --limit 1` is 3.4–3.6× faster (iter-259/260); MDN disk is 1.1–1.3× faster than v0.20.0 (the lead's 0.88 s single run vs 0.56 s best-of-3 is run-to-run variance, both under the 1.15 s baseline); GH Docs `links fix` within noise.
