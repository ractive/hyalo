# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Maintained with
`hyalo lint --profile changelog` and `hyalo changelog release`/`add`.

## [Unreleased]

### Changed

- **Loading a `.hyalo-index` snapshot no longer decodes the BM25 inverted
  index.** On a MDN-scale vault (14 375 entries, 116 MiB index) the BM25
  section is 76 % of the file, and a `find` with no text query spent ~230 ms of
  its ~400 ms walking postings it never read. The snapshot envelope is now
  visited by hand and the parse stops at the `bm25_index` key, leaving the
  section undecoded until something actually searches text. Measured on that
  vault, release build, median of 7 runs: `find --limit 1 --index` **396 ms →
  151 ms (2.6x)**; a text `find <query> --index`, which does decode the
  section, is 399 ms → 402 ms — inside run-to-run noise. The on-disk format is
  unchanged: indexes written before this release load unchanged, and indexes
  written after it are read by the previous release, verified both directions.
  Mutating commands (`set`, `remove`, `append`, `task toggle`, `mv`,
  `lint --fix --index`) force the section before re-saving, so none of them can
  drop the search index from a snapshot they only meant to patch.

- **`hyalo help <cmd>` now prints the short `-h` page, not the long `--help`
  one.** `hyalo help find` was 28 701 bytes where `hyalo find -h` is 2 992 —
  a 9.6x tax on the phrasing agents reach for first. The two are now
  byte-identical, and a subcommand path works (`hyalo help task toggle`).
  Because the forward is an argv rewrite rather than a page rendered from a
  `Help` arm, it inherits everything the short page is assembled from — the
  collapsed globals pointer, `find`'s composed examples, the footer naming
  `hyalo find --help` for the long form — and it inherits clap's did-you-mean
  for an unknown name, which the generated `help` subcommand never had:
  `hyalo help fnd` now suggests `find`. The long page is unchanged and one
  line away.
- **The envelope's `dry_run` claim is now true.** `hyalo --help`'s RESULTS
  CONVENTIONS said "every mutating command reports `dry_run` and
  `skipped_count`"; measured across all 23 mutating commands, 18 reported
  neither or reported the same fact under a different name (`apply`,
  `applied`). Every mutation result that is an object now carries `dry_run` —
  added to batch `mv`, `new`, `types remove`, `madr toc`, `okf index`,
  `okf log`, `changelog add` and `changelog release`. `apply`/`applied` are
  kept and are always its exact inverse, so nothing that reads them breaks.
  `skipped_count` is documented for what it is: bulk-family-only (`set`,
  `remove`, `append`, `properties rename`, `tags rename`), because a
  single-target command has no scanned-but-unchanged set. `task toggle`/`task
  set` return an array of per-task records with no top-level object and are
  named as the exception; their dry-run records are the ones carrying
  `old_status`.
- The root `-h` command group labelled `config and scaffolds (write
  .hyalo.toml)` now reads `setup (writes config/index, not your notes)`:
  `create-index`/`drop-index` write a snapshot index, not `.hyalo.toml`, and
  what a caller needs from that label is whether the group can touch their
  markdown.

- **BREAKING: an explicit `--fields` is an exact projection.** `--fields
  title` used to return `{file, modified, size, lines, title}` — the four
  "structural" keys were unconditional, so a projection could never cost less
  than they did, and there was no way to ask for just the paths and one
  property. `file` is now the only key no projection can drop: it names the
  result. `modified`, `size` and `lines` are ordinary members of the *default*
  set — present when no `--fields` is given, because they are cheap and are the
  inputs an agent uses to choose its next call (`read --lines`, recency) — but
  dropped when an explicit `--fields` does not name them. So `--fields title`
  is `{file, title}`, `--fields size,lines` is `{file, size, lines}`,
  `--fields file` is `{file}`, and `--fields all` is unchanged. Filter
  auto-includes still add on top of whatever set is in force. A saved view's
  pinned `fields` behaves exactly like an explicit `--fields`, and a CLI
  `--fields` now *replaces* that pin instead of extending it — extending it
  meant a view could only ever be widened, never narrowed. `--format text`
  renders exactly the projected set: with none of the three present, a result's
  header is the quoted path alone. On this project's own knowledgebase,
  `--fields title --limit 50` drops from 24.7 KB to 7.0 KB (−71 %).
- **BREAKING: a non-string frontmatter `title` is reachable again.** Only a
  YAML string promoted to the `title` field, so `title: 42`, `title: 1.0`,
  `title: true` and any unquoted date were invisible to `--fields title`,
  `--title` and `--sort title` — the item fell back to the first H1 and the
  authored value was reachable only through `properties`. YAML's type inference
  is an accident of the syntax, not an authoring choice: every scalar now
  promotes, stringified as written (`42`, `1.0`, `2026-08-30`, `true`), with
  the typed value still under `--fields properties-typed`. Null, empty and
  whitespace-only titles count as absent (H1 fallback) and stay in
  `properties`. A list or map has no honest string form, so it does not
  promote: the item's `title` falls back to the H1, the raw value stays in
  `properties.title` rather than being stripped as a consumed promotion, and a
  new default-on lint rule `HYALO007` (`title-not-scalar`, warn; error under
  `--strict`) reports it — `title: [Draft] Notes` is almost always a quoting
  typo.
- **One list of global flags, and it is the same list everywhere.** The set was
  written down three times — the root `-h` GLOBAL OPTIONS, the root `--help`
  COMMAND REFERENCE block, and the 52 `Global: …` pointer lines — and the three
  disagreed: the pointer omitted `--dir`, the reference omitted `--hints`. Both
  renderings are now generated from one table. The contradictory
  "Per-subcommand index flags" block is gone: `--index`/`--index-file` are not
  global and already appear in the Options block of every subcommand that reads
  a snapshot index. `--hints` is hidden from `-h` (it forces on what is already
  on; `--no-hints` next to it says which way the default points) and stays
  documented in `--help`.
- **`--dry-run`, placeholders and one-line help read the same across all 52
  pages.** The nine different phrasings of `--dry-run` collapse to one;
  `<PATTERN>`/`<RECENT>`/`<THRESHOLD>`/`<MIN_CONFIDENCE>`/`<IGNORE_TARGET>`/
  `<EXCLUDE_TITLE>`/`<EXCLUDE_TARGET_GLOB>`/`<PROFILE>` become
  `<GLOB>`/`<N>`/`<SUBSTR>`/`<TITLE>`/`<NAME>`, which also lets `links fix -h`
  and `links auto -h` render in the compact single-line layout the other 50
  pages use; `summary -n` loses its duplicated `(default: 10)`; and `read`,
  `task read/toggle/set` and `backlinks` get the same one-line
  `--file`/`--glob`/`--files-from` help `find` has, down from the three-to-five
  rendered lines each was wrapping to.

- **BREAKING: `find` returns a compact result shape.** The default field set
  was `properties, tags, sections, links` on every item, so a 20-file listing
  of this project's own knowledgebase was 73 KB of JSON — nine times the 8 KB
  the same query costs with `--fields properties`, nearly all of it document
  structure nobody had asked for. The default is now `file`, `modified`,
  `size`, `lines`, `title`, `properties` and `tags` (11.9 KB for that same
  listing). `sections`, `tasks`, `links`, `backlinks` and `properties-typed`
  are opt-in — `--fields all` restores the previous shape and then some — but
  a filter that implies a field still returns it with no flag: `--section`
  adds `sections`, `--task` adds `tasks`, `--broken-links`/`--dead-end` add
  `links`, `--orphan` adds both, and `--sort links_count`/`backlinks_count`
  now return the field they rank on instead of computing it invisibly.
  `title` is promoted: it joined the default set and is no longer repeated
  inside `properties`, so read it as `.results[].title`
  (`--fields properties` on its own still carries the raw property). Sorting
  and filtering are unaffected — `--sort property:title` and
  `--property title=…` compare exactly what they compared before.
- **`size` and `lines` on every `find` and `read` result.** Nothing in a
  result used to say how big a file was before you read it. Both numbers are
  now always present, count the whole file (a `read --section` reports the
  file's totals, not the slice's), are identical between a disk scan and an
  `--index` snapshot, and count CRLF and LF documents the same. `read` over
  8 KB offers a line-range read and a section listing instead of another
  whole-file pull, and `--format text` prints whichever of them the payload
  carries in each result's header line, plus a `fields:` summary naming what
  the payload carries and what `--fields all` would add. Snapshot indexes written by
  0.21.0 and earlier report `0` for both until the next `create-index`.

- **`-h` is a short page again, and every command's page names its
  capabilities.** Measured on 0.21.0, `hyalo -h` was 7.7 KB and `hyalo find -h`
  12.3 KB, because no flag's documentation had a first-line/rest split (so
  clap's short help printed whole paragraphs) and the ~1.9 KB global-options
  block was repeated on all 27 subcommands. Every `#[arg]`/`#[command]` doc
  comment now leads with a one-line summary and keeps its full text as
  `--help`'s long help; on a subcommand, `-h` stands in for the global block
  with a single `Global: --dir --format --jq … — see hyalo -h` line. `hyalo -h`
  groups the commands by intent (read / write / config and scaffolds) with
  one-line descriptions that name the capability families, and its examples
  each compose two or three features instead of demonstrating one flag; the 40
  single-flag examples moved to `--help`. `find -h` groups its flags under
  Filters and Output and keeps the property-operator line, the dot-path note,
  the `--fields` values and the `--sort` keys — the four things agents were
  falling back to `grep` for. Result: `hyalo -h` 2.5 KB, `hyalo find -h` 2.9
  KB, every other subcommand under 2.3 KB, with `--help` unchanged in content.
  A new `check-help-drift` gate (3c/3d) and an e2e suite that walks the
  binary's own command list hold the ceilings.
- **A `find` that matches nothing now says what it ran and what to try.** Text
  mode prints `No results for --property status=x --tag y` instead of a bare
  `No results`, and both formats carry 1–3 hints: a did-you-mean over the
  values the property really has (fired only when the edit distance is small,
  so `status=draf` suggests `draft` while `status=nonexistent` does not), the
  observed values named inline, and the same query with its most selective
  filter dropped. The values come from the scan the query already paid for, so
  the empty path costs no extra I/O.
- **mdbook-lint bumped to 0.16.1.** `mdbook-lint-core` and
  `mdbook-lint-rulesets` move 0.16.0 → 0.16.1 (lockfile only; the manifest
  requirement `"0.16"` is unchanged). The release carries upstream
  [#496](https://github.com/joshrotenberg/mdbook-lint/pull/496), which fixes
  MD047 on CRLF files: trailing terminators are counted with CRLF as one unit
  (so a CRLF file with several trailing blank lines is now detected at all),
  and the missing-final-newline fix inserts the file's own terminator instead
  of a hard-coded LF. Where a file's endings are mixed, the terminator of the
  line immediately before EOF wins.
- Link resolution folds ASCII case on every platform, not only on a case-insensitive filesystem (DEC-267). `links fix --case-insensitive` no longer changes what resolves; it only suppresses the cosmetic case-mismatch rewrite plans.
- lint: MD001 (heading-increment) is reported but no longer autofixable — renumbering a deliberate `######` caption rewrites authored structure (DEC-272). `lint-rules list` shows `autofixable: false`
- lint --fix: each deferred fix now prints `conflict <RULE> line <N>: range overlap with <RULE>` in text output (first 20 per file, `--detailed` for all) instead of a bare `conflicts N`; JSON `conflicts[]` entries gain `line` and each file gains `conflicts_total`
- find: every --sort key now orders ascending and --reverse inverts it; 'backlinks_count' and 'links_count' no longer sort descending, so '--sort backlinks_count --reverse' is 'most linked first' (BEHAVIOUR CHANGE, DEC-273). 'score' still ranks best-match-first. Files whose sort property is missing or null always sort last, in both directions.
- find --property ordering comparisons (>, >=, <, <=) are typed: numeric when both sides parse as numbers, by date when both are ISO dates, textual only between plain strings; a value of any other kind never matches, so 'last>=2023-09-01' no longer matches the string '[[2022-04]]' (DEC-274).
- find --files-from returns the same 'results' array as --file: the files_missing, files_skipped_non_md and files_skipped_outside_vault counters moved from inside 'results' to top-level envelope keys, present on every find (zero when --files-from was unused), and results is no longer promoted to {files: [...]} (SHAPE CHANGE; same for lint, task and read --files-from).
- Per-file YAML parse diagnostics are collapsed into one end-of-run stderr line naming the count and `hyalo lint --rule HYALO005` (DEC-278); `-q` silences it and `[scan] verbose_skips`/`RUST_LOG=hyalo=debug` restores the detail
- A malformed `.hyalo.toml` now exits 1 for gate commands (`lint`, `find --strict`, `views run`) as well as writes (DEC-279); plain reads keep answering with the `-q`-proof warning
- A `--index` read that names its files stat-refreshes just those entries instead of warning that the whole index is stale (DEC-280)
- `types set --required K` infers the property type it auto-declares for K from the values the vault already holds for that key, instead of always declaring `string`
- A title with no frontmatter `title` and no H1 is now the filename stem, as in Obsidian (DEC-283)
- `links auto` holds back common-word and run-dominating candidate titles by default; opt out with `--no-warn-common-titles` or `[links.auto] exclude_titles` (DEC-286)
- A file named with `--file` / positionally / via `--files-from` is linted even when `[lint] ignore` matches it; `--glob` and the bare sweep still honour the list (DEC-284)
- `hyalo new` scaffolds a required number/date/boolean with no schema default as an empty value instead of 0/today/false, so `lint` reports it (DEC-285)
- Text-mode errors use the lowercase `error:` prefix, matching clap and anyhow
- `hyalo config` reports the effective output format plus `format_source`, and an effective `hints` boolean
- `lint --format github` summary counts files checked: `N errors, M warnings in K of T files checked`
- `set`/`append` with `--validate` (or `[schema] validate_on_write`) now refuse with exit 1 when `[schema]` exists but cannot be loaded, instead of falling back to an empty schema and writing unvalidated (DEC-290). Writes without `--validate`, and every other command, are unaffected.
- `mv --on-conflict` is a validated choice (`error` | `skip`): an unknown policy is now a usage error instead of parsing and silently behaving as `error`, and single-file mode honours `skip` (the source stays put, reported under `skipped`, exit 0). The batch collision error distinguishes two sources sharing a destination from a destination that is already taken.
- Hints on an indexed run carry `--index` / `--index-file` wherever the hinted command accepts it, so a follow-up query stays indexed instead of rescanning the vault (UX-2).
- `links fix` prints the `site_prefix` diagnostic before fuzzy scoring and skips scoring the site-absolute links once 500 or more of them resolve nowhere (UX-3); `find --broken-links` points at `site_prefix` instead of `links fix` when every broken link is site-absolute.
- The `links auto` stop-list note caps its `--exclude-title` list at the five titles it says it shows and states that flags ADD to the built-in list while `[links.auto] exclude_titles` REPLACES it — an empty list now switches the stop-list off (UX-11, BUG-23).
- Exit codes are now a documented taxonomy (DEC-307): 0 answered, 1 every hyalo-own user error rendered through the JSON envelope, 2 clap usage and internal errors. `find a b`, a bad `--glob`, an unreadable `--files-from` list, `create-index --output` into a missing directory, an unknown `init --profile` and `deinit --dir <nonexistent>` all exit 1; `okf index` without `--apply` exits 0 and reports drift as `results.changed` (UX-1, BUG-25, UX-18, UX-20).

### Removed

- **The last downstream compensation for an upstream mdbook-lint bug.**
  `md047_fix` in `hyalo-mdlint` — which computed MD047 fixes locally for CRLF
  bodies, the one documented exception left after iteration 196 — is deleted
  along with its dispatch branch. MD047 now goes through the same generic
  `convert_fix` translation as every other rule, so `hyalo-mdlint` carries no
  rule-specific fix overrides. No user-visible behaviour change: the CRLF and
  mixed-endings MD047 tests written against the override keep their expected
  outputs against 0.16.1 (re-pointed at the engine, one extra convergence
  assertion) and stay as the regression check.
- The undocumented '=~' property-filter operator: 'K=~/pat/' is now a hard error naming '~=' (BREAKING; it was only ever accepted because '=' split first and '~/pat/' became a literal value, which matched YAML nulls). Empty property regexes ('K~=', 'K~=//') and empty --fields selections ('--fields ""', '--fields ,') are rejected too (DEC-276).

### Fixed

- **`hyalo init --dir <tree-outside-CWD>` wrote a config it then refused to
  read.** The `.hyalo.toml` landed in the *current* directory with an
  absolute `dir`, which a project-local config is not allowed to set
  (iter-221 H-1, tightened in iter-243/244) — so the very next hyalo run
  rejected the file `init` had just written. `--dir` now decides the project
  root as well as the vault: a vault at or below CWD keeps `.hyalo.toml` here
  and records `dir` relative to it, while a vault outside CWD makes that tree
  its own root, written there with `dir = "."`. Containment is decided on
  canonicalized paths, so an absolute spelling of a subdirectory, a
  `sub/../kb` round-trip and macOS's `/tmp` → `/private/tmp` symlink all
  resolve to "inside" — matching what the reader's own bounds check does
  (DEC-261).
- **`hyalo --dir <other-tree> deinit` deleted the *current* tree's
  integration files.** `deinit` ignored `--dir` entirely and always targeted
  CWD, so a probe aimed at a throwaway vault removed this repo's own
  `.hyalo.toml`, `.claude/CLAUDE.md` and three `.claude` symlinks — under a
  summary whose dozen interleaved `skipped … (not found)` lines read like a
  no-op at a glance. `deinit` now follows the same root rule as `init` and
  can no longer touch a tree the user did not name. Any run whose root is not
  CWD leads its summary with a `target <path>` line (DEC-261).
- **`init`/`deinit` ignored `--format json`.** Both printed their text
  summary regardless, so an agent piping JSON got unparseable output. They
  now answer an explicit `--format json` (or `--jq`, which implies it) with a
  minimal envelope — `results.command`, `results.root`, `results.actions[]`
  of `{action, target, detail?}`, plus a hoisted top-level `dir` for `init` —
  and refuse `--format github` with the same message every other non-lint
  command gives. They deliberately stay text when merely piped: the summary
  is a setup progress report, not a result set, so `hyalo init | tee
  setup.log` keeps working (DEC-262). Text and JSON are rendered from one
  `Report` struct and cannot drift.

- **Building the wikilink stem index was quadratic in vaults that reuse one
  basename.** `CaseInsensitiveIndex::insert` deduped by linear-scanning the
  *stem* bucket, so a docs tree naming every page `index.md` put its whole
  file list in a single bucket: on MDN's 14 399 files that was ~104 million
  string comparisons, 62.4 ms, every time a command needed outbound-link
  resolution. Dedupe is now decided against the path bucket, which holds only
  the case-variants of one path and is written in the same call — 62.4 ms
  becomes 4.2 ms. The visible symptom was `find --fields all` costing ~20%
  more wall time than the default field set on an indexed vault even at
  `--limit 1` (0.448 s vs 0.371 s); it is now 0.368 s, i.e. free. The whole
  delta was this one call — `sections`, `tasks`, `backlinks` and
  `properties-typed` were never the cost. Unindexed vaults share the same
  code path, so `links fix`, `--broken-links`, `--orphan` and `--dead-end`
  improve too.
- **A mutation that changed nothing left the index describing a file that no
  longer existed.** `set`/`append`/`remove` only touched the snapshot when they
  actually wrote, so `set note.md --property status=completed --index` on a file
  whose `status` was *already* `completed` reported `0 modified` and left the
  entry exactly as the last `create-index` saw it — including its cached body
  tokens, `links`, `size` and `lines`. Append a paragraph by hand first and
  `find <word-from-that-paragraph> --index` kept answering `0` while a disk scan
  found it. All three commands now repair the entry of every file they read
  whose `(mtime, size)` no longer matches the snapshot, whether or not the write
  turned out to be a no-op — a full rescan plus link-graph refresh, the same
  repair an externally rewritten body already got from `links fix --apply`. The
  check reuses the `stat` the concurrent-write guard already takes, so a batch
  where nothing is stale costs nothing extra, and `--dry-run` still writes
  nothing.
- **`read` and `find` disagreed about what invalid UTF-8 costs you.** `read`
  substituted `<line skipped: invalid UTF-8 (lossy in search; fix encoding to
  read)>`, promising search still saw the line, while `find <text>` dropped the
  whole file with the raw io message `stream did not contain valid UTF-8`. Only
  the regex path (`find -e`) is genuinely lossy. Both surfaces now print the
  same sentence — the encoding is the problem, full-text search excludes the
  file, and `-e` still matches it — from a single shared constant, with an e2e
  that pins the wording *and* checks both halves of the claim against real
  behaviour.
- **`hyalo new --property k=v` failed without saying where properties are
  set.** `new` scaffolds strictly from the type's schema, and clap's
  nearest-match tip pointed at `--type`. It now answers with the
  scaffold-then-set chain (`hyalo new --type T --file F && hyalo set F
  --property k=v`), `--tag` gets the same pointer, and `new --help` states that
  there is no `--property` flag and shows the chained example. No new flag: the
  gap was discoverability, not capability.
- **A bundled recipe that had been silently returning nothing.** The
  "planned items where all tasks are done" recipe in the `hyalo-tidy` skill
  reads `.tasks` but never asked for the field, so from the moment `tasks` left
  the default set it matched nothing on every vault — no error, no warning, just
  a wrong answer in a recipe agents are told to trust. It now passes
  `--fields tasks`, and an e2e walks every bundled skill/template/rule file to
  fail any `hyalo find … --jq` line that reads `tasks`, `sections`, `links`,
  `backlinks` or `properties_typed` without a `--fields`, a `--view`, or a
  filter that auto-includes it.
- **Sixteen `-h` lines that ended mid-sentence.** The iteration-251 split moved
  the detail of each doc comment into a second paragraph, but sixteen of them
  were cut at a line break rather than a sentence boundary, so `-h` shipped
  lines like `Validate new values against the schema from .hyalo.toml; reject
  writes that would` and `Scaffold a preset vault flavour (okf, madr, skills,
  changelog) by`. All sixteen are rewritten as whole sentences, and a new
  `check-help-drift` gate (3e) plus an e2e now fail any short-help entry that
  ends on a dangling word or `,;:`, or that spans more than two rendered lines.
- **`views run --filenames-only` / `--filenames0` were advertised and ignored.**
  Both appear in the `views run -h` Output group, but the flags were
  destructured away, so `views run gate --filenames-only` printed the full JSON
  envelope while `find --view gate --filenames-only` printed bare paths. They
  now route through the same projection as `find`, applied after the `--strict`
  check so a filename list can still be a CI gate.
- **Help text that described the pre-0.22 result shape.** The `find` headline,
  the `find --help` opening paragraph, the `--fields` help, the root `--help`
  JSON cookbook (`find`, `read`, `task read/toggle/set`) and the bundled
  `SKILL.md`s all still described the old shape — the cookbook showed `title`
  inside `properties` and no `size`/`lines` at all. All are regenerated from
  real output, and an e2e now asserts the cookbook's key lists equal the live
  ones. `--orphan`/`--dead-end` say they auto-include links *and* backlinks
  (which is what they do); `mv --property` says it takes the same syntax as
  `find --property` (which it does — `~=`, `!K` and dot-paths all work);
  `task --line` states that its numbering is file-absolute with frontmatter
  counted, and `read --lines` that its numbering is body-relative, each
  cross-referencing the other; `lint --help`'s `--fix-rule` example includes the
  `--fix` it requires; `find --help`'s PROJECTIONS paragraph no longer prints a
  literal NUL byte and newline into the terminal; and the rustdoc intra-doc link
  that was rendering as `[`crate::list_commands::LIST_COMMANDS`]` on ~15 pages
  is now an ordinary code comment.
- `hyalo-cli` crates.io publish (v0.21.0) failed at the tarball verify step because the pi integration files were embedded via `include_str!` reaching outside the crate into the top-level `pi-package/` directory, which `cargo package` cannot see. The four files are now vendored inside `crates/hyalo-cli/templates/pi/`, kept in sync with `pi-package/` by a new `check-pi-package-sync` CI gate (`just sync-pi-package` to fix drift).
- Any `scheme:` target (`obsidian://`, `mailto:`, `file://`, `zotero:`) is now an external link, not a broken one — a Windows drive letter (`C:\x`) still is not. External links are inventoried with `kind: "external"` and their URI kept verbatim, query string included.
- Link targets with an explicit non-`.md` extension (`![[img.png]]`, `[[Books.base]]`) resolve against every vault file the way Obsidian does — by unique basename, by path, and relative to the source folder — and are reported as `kind: "attachment"` instead of broken.
- `[[target\|alias]]` — the escaped alias pipe Obsidian writes inside markdown tables — now splits into target and alias, and `mv` / `links fix` keep the `\|` bytes so the table row survives the rewrite.
- `links fix` never matches a target across an explicit extension (DEC-266), so a broken `Companies.base` is no longer offered `Company Template.md`, and no fuzzy candidate is reported at confidence 0.0.
- `hyalo mv` now rewrites frontmatter wikilinks that name the moved file by bare stem (`categories: ["[[Books]]"]` → `Categories/Books.md`), instead of reporting `links updated: 0` and leaving the reference dangling. Quoting style and every other byte of the block are preserved; a `[[…]]` spanning a line break is left alone, warned about on stderr and listed under `frontmatter_links_skipped`.
- A frontmatter list of wikilinks renders as `["[[Futurism]]", "[[Nonfiction]]"]` in text output instead of the unreadable `[[[Futurism]], [[Nonfiction]]]`; plain lists keep their compact unquoted form.
- lint: MD018 no longer rewrites an Obsidian tag line (`#todo`) into a heading; MD034 no longer wraps a URL that is already a link destination; MD042 accepts an image as link text (`[![](img.png)](https://…)`) (DEC-271)
- find --filenames-only no longer emits a trailing blank line, so 'find --filenames-only | wc -l' equals 'find --count' (also fixes 'views run --filenames-only').
- `links auto --index` no longer aborts when its refresh pass meets a file with unparsable frontmatter; it skips and counts it like the disk scan (BUG-8)
- `create-index` keeps invalid-UTF-8 files out of the BM25 corpus, so `--index` scores match the disk scan's exactly, and reports them under `warnings` (BUG-14)
- `properties rename` renames the key in place: same position in the frontmatter block and the value's exact source text (quoting, spacing, comments, list indentation); an empty `rating:` becomes an empty `score:` instead of `score: null`
- `tags rename` on a parent tag renames its whole subtree (Obsidian semantics): `--from music --to audio` also moves `music/genres`, works when only children exist, and never matches `musical`; every renamed tag is listed in the text output and under `renamed_tags` in JSON
- `hyalo properties --index` and `hyalo tags --index` are accepted on the bare command, not only on the `summary`/`rename` subcommand
- Schema type binding accepts `type:` as a `[[Wikilink]]` or a one-element list of either a string or a wikilink, so `types set`, `lint` and `set --validate` apply to Obsidian vaults that write `type: ["[[Authors]]"]`
- `hyalo summary` lists a mixed-type property once with a type breakdown instead of once per type, so its property count is the number of distinct names and matches `hyalo properties --count`
- `hyalo read --frontmatter` returns the frontmatter block's own bytes instead of re-serialised YAML; JSON adds `frontmatter_raw` beside the parsed map
- An unquoted multi-word `find` query (`hyalo find dataview plugin`) now names the quoted command instead of failing with `file not found`
- The zero-result notice prints before the hints that explain it, and the hint block no longer opens with blank lines
- `find --help` COMMON MISTAKES no longer claims `--property title~=` searches only frontmatter; help pages carry no non-breaking spaces
- `hyalo index` suggests `create-index` instead of clap's nearest match; `types list` on an unconfigured vault says `No types configured`
- The slow-query hint suggests `--index` when a `.hyalo-index` snapshot already exists, instead of rebuilding it
- Multi-line values in text output are indented under their key, so continuation lines cannot be read as new keys
- `types remove` on an undeclared type explains that lint's errors come from `[schema.default]`, not from a type entry
- A `set`/`remove`/`append --index` run that names its files no longer warns about staleness it repairs itself
- The root `-h` banner no longer tells you not to `cd` into a vault configured as `dir = "."`
- SCHEMA-group constraint violations (`pattern`, `item_pattern`, `object-list`) no longer report `autofixable: true` when `--fix` has no fixer for them
- `mv` now reports a frontmatter wikilink that spans a line break even when the file holding it has no other link to the moved target — such a link is not a graph edge, so the backlinks graph alone never surfaced it (DEC-288)
- `mv` flags an ambiguous bare `[[stem]]` between two same-stemmed files even when neither sits at the vault root; which candidate was being moved no longer decides whether the warning appears (DEC-288)
- `lint --fix` no longer swallows an HTML tag into MD034's autolink: `https://…/x<br>` fixes to `<https://…/x><br>` instead of corrupting the markup (DEC-289)
- MD047 no longer reports a frontmatter-only file as missing a trailing newline (DEC-289)
- Frontmatter closes only on a column-0 `---`: an indented `  ---` inside a block scalar no longer truncates the block (keys after it were silently dropped and the next `set` overwrote the body). A malformed block is now reported as unclosed / HYALO005 (DEC-293).
- `set`/`append` never emit a block scalar containing a `---` or `...` line; such a value is written as a double-quoted scalar that round-trips (DEC-293).
- `properties rename --from '' ` / `--to ''` are refused with exit 1 instead of giving every file an empty-named property.
- MD031 no longer proposes a blank line at the opener of a fence that never closes — the fix landed inside the code sample.
- No prose rule (MD019 among them) fires inside a fenced or indented code block or an HTML comment; MD010, MD031, MD040, MD046, MD047 and MD048 keep checking them on purpose.
- `links fix` no longer reports a case mismatch for a site-absolute link carrying the configured `site_prefix`, and never appends `/index` or `.md` to a link form that lacked it (DEC-295).
- `mv` applies its ambiguity guard to frontmatter links too: a bare `related: "[[a]]"` matching two files is skipped and reported with its property, not rewritten.
- A one-element list `type: ["[[Author]]"]` no longer fails the implicit string constraint that every declared schema type carries — the shape Obsidian's property editor writes now lints clean wherever DEC-281 binding already accepted it.
- An anchor-only markdown link `[text](#frag)` is reported with `kind: "markdown"` and its link text, not as a wikilink — vaults with no wikilinks at all (MDN, GitHub Docs) claimed thousands.
- A wikilink capture stops at the first stray `]` or nested `[[`, and never starts inside `[[[`: `see [[Leah] here and [[Target]]` is one link to `Target`, and a YAML flow list `related: [[[a]], [[b]]]` yields both links instead of `[a`.
- A markdown destination in angle brackets whose text is a parenthesised URL — `[y](<(https://…)>)` — is external: never broken, never fuzzy-matched.
- `![alt](img.png)` is inventoried like `![[img.png]]` (DEC-297), so a missing image surfaces in `find --fields links`, `--broken-links` and `summary` instead of being dropped at extraction.
- The DEC-268 heading suggestion folds `-`, `_` and space (DEC-298), so an underscore-slugged fragment such as `#Browser_compatibility` now suggests the `Browser compatibility` heading it prefixes.
- `find` no longer answers a *named* file with an empty success: a `--file` or positional path whose YAML frontmatter will not parse exits 1 with the diagnostic (a `--files-from` list keeps batch semantics and counts it).
- `find --index --file <path>` reads a note the snapshot has never seen from disk instead of returning `results: []`, so a file created since the last `create-index` is never invisible.
- `find --file`/`--glob` with `--broken-links` (or `--fields links`) keeps `broken_anchor` and `suggested_fragment`; the four ways of selecting one file now return identical link JSON.
- `lint --rule <ID>` no longer reports frontmatter parse errors under an unrelated rule — `--rule MD018 --count` on a vault of unparsable templates answers 0 plus the skip summary, and HYALO005 still reports them.
- The `--index` staleness probe now sees an in-place overwrite: when the directory-mtime probe is clean, each indexed file's recorded mtime is compared against disk and the first drift is named in the warning.
- `summary --index` reports the same `excluded` count as a disk scan — the snapshot records how many files `[scan] exclude` dropped when it was built, and the patterns that dropped them.
- `mv` resolves the destination exactly like the source, so `hyalo mv kb/a.md kb/sub/a.md` run beside a `dir = "kb"` vault lands at `kb/sub/a.md` instead of creating `kb/kb/sub/`. All four destination forms are affected; a missing `--to dir/` is reported as a missing directory rather than `did you mean dir/.md?`.
- Batch `mv` runs the split-frontmatter-link sweep single-file `mv` has run since iteration 269 — once per batch — and reports each move's own `frontmatter_links_skipped`.
- Hints: `summary`'s site-URL diagnostic names the real `site_prefix` key (BUG-22); a zero-result `find` distinguishes a key the vault does not have from one whose values simply do not match, listing the values with counts (BUG-17); `links fix` no longer warns "stripped 0 of N" for a site-absolute link that resolved (UX-22).
- Help text: the `types` subcommands no longer leak 12-space doc-comment indentation into `--help` (UX-7), and `check-help-drift` gate 3f fails on any future leak; the unknown-flag tip consults the invoked subcommand's real flags instead of recommending a `--property` it does not have (UX-8); the `find` text footer no longer lists `score` and `matches`, which `--fields` rejects (BUG-27).
- `--property` rejects an empty operand (`K=`, `K>=`), a second `=` (`a=b=c`) and an empty name with exit 1 instead of matching nothing in silence (UX-21); the mixed-type sort warning is computed over the whole sorted set, so `--limit 2` warns when `--limit 0` does (UX-4); `--sort title` collates case-insensitively and skips leading punctuation (UX-15); an H1-derived title has its HTML comments stripped (UX-16).
- `lint-rules show` reports only the dimensions an override actually sets, never `enabled: null` (UX-14); `task read`/`task toggle` JSON strips the trailing carriage return on CRLF files (UX-17); an empty required typed placeholder reports one error rather than two (UX-6); `types set` says on stderr when it enables `validate_on_write` as a side effect (UX-19).
- The shipped `--jq` recipes no longer use jq's `IN(...)`, which hyalo's own jq engine does not implement, and the new `xtask check-jq-recipes` gate executes every recipe in every shipped document against the vault (BUG-29). `hyalo config` reports `links.case_insensitive` (BUG-19), and an empty `--files-from` list warns instead of exiting 0 in silence (UX-9).

### Added

- **A zero-result `--property K~=RE` query now points at body search when the
  same regex matches body prose.** `hyalo find --property 'title~=/DEC-25/'`
  against a decision log whose `DEC-NNN` ids are `##` headings was correct and
  useless; the empty result now leads with `hyalo find -e 'DEC-25'`. The hint
  is emitted only after a bounded probe (512 files / 8 MiB, first match wins)
  confirmed the suggested command returns something, and only on the
  zero-result path — no new flag, and no cost on any query that matched.
- Every entry in `find --fields links` carries `kind`: `wikilink`, `embed`, `markdown`, `external` or `attachment`. External and attachment links never count as broken and are not graph edges for `--orphan`/`--dead-end`.
- A broken `#anchor` that is the prefix of exactly one heading in the target file carries `suggested_fragment` with the full heading text (DEC-268) — reported, never applied automatically.
- Frontmatter wikilinks are first-class links: a `[[wikilink]]` in any frontmatter value (`categories:`, `type:`, a nested map — quoted or bare) is now a graph edge for `backlinks`, `find --orphan`/`--dead-end`/`--broken-links`, `summary.links`, HYALO006 and the `links_count`/`backlinks_count` sort keys, reported with `kind: "frontmatter"`, the originating `property` and its frontmatter line (DEC-269).
- `[links] frontmatter = false` in `.hyalo.toml` narrows frontmatter link scanning back to `related`/`depends-on`/`supersedes`/`superseded-by`; `hyalo config` reports the effective value under `links.frontmatter` / `links.frontmatter_properties`.
- `hyalo mv` text output prints `files updated: N, links updated: M` under the `Moved …` line in both single-file and batch mode, dry runs included, so a rewrite that matched nothing is visible.
- `hyalo set K=<scalar>` on a property that holds a YAML list reports the type change — a stderr note pointing at `hyalo append`, and the affected files under `list_collapsed` in JSON (DEC-270).
- find --property value syntax: 'K=null' / 'K!=null' match a property present with (or without) a YAML null, and 'K=[]' / 'K!=[]' an empty list (DEC-274). A list containing a null does not match 'K=null'.
- find --fields accepts 'properties_typed' as well as 'properties-typed'; the JSON key stays the snake_case 'properties_typed' (DEC-275).
- `[scan] exclude` in `.hyalo.toml` hides matching files from every command and every `--index` read (DEC-277); `hyalo config` reports it under `results.scan.exclude`
- `summary` reports `results.files.skipped` and `results.files.excluded` (text: `Files: 75 (28 skipped, 0 excluded)`), with per-directory skipped counts
- summary -h/--help now list the JSON result keys with a worked --jq example
- `hyalo new --dry-run` prints the scaffold and writes nothing (DEC-285)
- `find` reports `title_source` (property | h1 | filename) alongside `title` (DEC-283)
- `links auto` reports `default_excluded_titles` / `default_excluded_mentions` (DEC-286)
- Schema: `object-list` property type with `required-keys`, `allowed-keys`, `key-patterns` for lists of maps; lint reports the item index and key, and a plain-string item gets a `- ref: <value>` fix-it hint
- `lint` honours markdownlint's suppression comments — `markdownlint-disable`, `-enable`, `-disable-line`, `-disable-next-line`, `-disable-file`, `-enable-file` — with rule ids or aliases (DEC-294).
- Frontmatter `aliases:` resolve wikilinks (DEC-296): `[[Leah]]` finds the note declaring `aliases: [Leah]`, reported as `via: "alias"`. A filename always wins, a shared alias is ambiguous, matching folds case, and alias links are real graph edges. Opt out with `[links] aliases = false`.
- `links fix` reports `emitted_target` — the exact link text `--apply` writes — on every fix bucket, filled by the same planning pass in both modes so a `--dry-run` preview is byte-accurate (`new_target` stays vault-relative).
- `hyalo lint --rule SCHEMA` / `--rule-prefix SCHEMA` run the frontmatter schema pass alone, and `lint-rules list`/`show` carry a non-configurable `SCHEMA` row (UX-5). `lint --fix` JSON gains `rules_fixed: {rule: n}` (UX-13).

## [0.21.0] - 2026-08-28

### Added

- **Iteration 245 — deferral carry-overs.**
  - **UX-3 follow-up: dot-path property filters descend sequences.** A
    frontmatter list of maps (`contacts: [{name, email}, …]`) is now
    traversable: `--property 'contacts.email=x'` auto-descends into every
    element and matches when any of them carries that value, while a numeric
    segment pins one element (`contacts.0.email`). The collected values are
    returned as a list, so the established list semantics apply unchanged
    (`=`/`~=` match when any element matches, `!=` when none does, and a bare
    key exists when at least one element yielded a value). Applies to every
    filter consumer, including `--where-property` on the mutating commands.

- **Iteration 244 — index remaining deferrals.**
  - **UX-3: nested dot-path property filters.** `--property 'a.b=v'` now
    traverses nested YAML mappings (`contact.email=team@example.com`
    matches `contact: {email: …}`) across `find` and every filter consumer;
    a literal dotted key in a flat map still takes precedence over
    traversal.
  - **UX-6: case-insensitive link resolution mode.** `links fix
    --case-insensitive` (or `[links.case_insensitive] resolve = true` in
    `.hyalo.toml`) treats links that resolve only by case folding as
    resolved rather than fixable, so MDN-style vaults with case-folded
    directory layouts no longer offer a `link-case-mismatch` rewrite plan
    per link in `links fix --dry-run`.

### Changed

- **pi package 0.1.1.** Root `package.json` and `pi-package/package.json`
  bumped to 0.1.1 (DEC-101 version discipline for the root-manifest change
  in PR #279), so `pi update --extensions` on a pinned git install can
  observe a pushed change.
- **`hyalo summary --format text` no longer prints a `kb dir:` banner on
  stdout.** The resolved vault dir moves to stderr as `note: kb dir: <path>`,
  so a text-mode report is data from its first line; `-q` suppresses the note
  and `--format json` still carries `dir` (DEC-247).
- **Internal: the three largest CLI modules are split into submodules.**
  `hints.rs` (5,059 lines), `commands/lint.rs` (4,005) and `output.rs` (3,744)
  become `hints/`, `commands/lint/` and `output/` directories. No behaviour,
  path or visibility changes — every item keeps the visibility it had in the
  single module.

### Fixed

- **Pre-v0.21.0 dogfood fixes (iter-249).**
  - **UX-1: stale-index probe was blind past the vault's top level.** The
    `index older than vault` staleness probe (`create-index`/`--index`)
    used to check only the vault root and its immediate subdirectories, so
    a file added or removed two or more directories deep — the common case
    on real vaults (`iterations/done/*.md` here, nearly every page on MDN
    and GitHub Docs) — never tripped the warning. The probe now walks
    directories recursively down to 3 levels (measured: an unbounded walk
    added ~65% to an indexed query on MDN's 14k-file tree, well past
    budget, so depth is bounded rather than unlimited), still without
    reading any file content. `create-index --help` and the `--index` flag
    help now describe the real depth and its two remaining blind spots
    (in-place edits of existing files, which never move a directory's
    mtime, and files added or removed inside a directory more than 3
    levels below the vault root).
  - **BUG-1 (carry-over): `task toggle`/`task set --index` BM25 parity.**
    `task toggle --all --index` and `task set --index` used to patch just
    the toggled tasks' status in place, leaving the entry's cached BM25
    tokens stale relative to the file's new body bytes; the journal now
    re-scans the mutated file the same way `set`/`append`/`mv`/`lint --fix`
    already do, so `find --index` scores stay byte-identical to a disk
    scan after a task mutation. `lint --fix --index` was audited too and
    already re-tokenizes correctly.
  - **UX-2: `links fix --apply --apply-fuzzy` text summary mislabeled
    applied fuzzy matches as excluded.** The `Low-confidence matches (…)`
    summary line always said "excluded from plain --apply", even when
    `--apply-fuzzy` had just written those matches to disk. It now reads
    "applied via --apply-fuzzy" when the flag was active, and keeps the
    original "excluded from plain --apply" wording otherwise. JSON keys
    are unchanged.
- **`read` no longer misreports invalid UTF-8 lines as oversized (iter-246,
  F-5).** A body line containing invalid UTF-8 was skipped with the
  "exceeds 1 MiB per-line limit" placeholder because the capped line reader
  folded both skip causes into one flag. `scanner::read_line_capped` now
  reports a distinct `LineOutcome::InvalidUtf8`, and `hyalo read` emits
  `<line skipped: invalid UTF-8 (lossy in search; fix encoding to read)>`;
  the oversized-line placeholder is reserved for real over-quota lines.
- **BUG-4 (carry-over): post-mutation BM25 parity.** After a mutation wave
  under `--index`, `find --index` scores are byte-identical to a disk scan
  without an intervening `create-index`: incremental re-scans re-tokenize
  bodies when the snapshot carries a BM25 index, and the journal flush
  rebuilds the persisted inverted index from (re-scanned ∪
  postings-reconstructed) tokens. BM25 result ordering is now deterministic
  (ties broken by path) on both the index and disk paths.
- **`new` link-graph upsert.** A file created by `hyalo new` now registers
  its template's outgoing wikilinks (frontmatter link properties such as
  `related`) in the persisted link graph, so `backlinks --index` sees them
  without a full rebuild — the last mutating write path without the iter-243
  upsert-with-links guarantee.

- **Index/disk parity bugfix wave (iter-243, dogfood v0.20.0).** Four
  bugs with one theme — `--index` output must be indistinguishable from a
  disk scan:
  - **BUG-1 (upsert on miss):** every mutating command (`set`, `set --tag`,
    `task toggle`, `append`, `remove`, `lint --fix`, `links fix --apply`,
    `mv`, `tags rename`) now inserts a full entry (frontmatter, tasks,
    links, graph edges) for a file the snapshot index has never seen —
    previously only files the index already knew were refreshed, so a file
    created by an editor/Obsidian and then mutated through hyalo stayed
    invisible to every indexed read and never contributed links to the
    graph. `links fix`/`links auto` additionally upsert unknown files during
    their pre-discovery heal pass, so broken links in un-indexed files are
    found and fixed under `--index` too.
  - **BUG-2 (`links fix` trust + `applied` semantics):** the pre-discovery
    heal (iter-241's detection half) now also covers files the index does
    not know, and `applied` in `links fix` output means "something was
    actually written", not "apply mode was requested": a `fixes: 0`
    `--apply` run reports `applied: false` in JSON and `Applied: no (no
    fixes written — nothing to apply)` in text (dry runs say
    `Applied: no (dry run)`).
  - **BUG-5 (backlinks order):** `backlinks` results are sorted by
    `(source, line)` on both the index and disk paths, so the two outputs
    are byte-identical after a mutation wave and stable across refreshes
    (the persisted graph's insertion order used to depend on which files a
    journal refresh happened to rewrite).
  - **BUG-4 (BM25 corpus drift, partially):** `create-index`'s BM25 body
    collection now includes code-fence delimiter lines (`` ```rust ``) and
    `%%` comment-fence lines, exactly matching what the disk-scan corpus
    builder tokenizes (`frontmatter::body_only`), so scores are identical
    between `--index` and disk scan on a fresh index (`TOKENIZER_VERSION`
    is bumped to 3 so pre-existing snapshots re-tokenize from disk).
    Remaining drift *after* mutations is timeboxed-out: the persisted
    inverted index cannot be cheaply updated incrementally (entries' tokens
    are stripped once an inverted index exists), so a rebuilt index is
    needed for exact post-mutation parity — see the dogfood note.

### Changed

- **Lint subsystem moved into `hyalo-mdlint`; one `MutationJournal` owns
  index maintenance (iter-226).** Two architecture refactors with no
  user-visible behaviour change (lint output is byte-identical, the snapshot
  format is untouched). The five conformance-profile linters (changelog /
  madr / okf / skills / github) plus their engines (`heading_grammar`,
  `link_lint`, `section_scanner`) and the schema-validation core of `lint`
  (per-file validation, auto-fix, `validate_constraint`) now live in
  `hyalo-mdlint` (`profiles::*`, `schema`), exposing an in-process lint API
  for library consumers — the CLI keeps flag parsing and output formatting
  only. On the index side, the three scattered index-refresh mechanisms
  (`save_index_if_dirty`, `tasks::patch_index`,
  `patch_index_for_modified_files`) are folded into a single
  `MutationJournal` every mutating command records through, flushed once per
  invocation — always refreshing the persisted entry *and* its link graph.
  This also fixes two latent stale-link-graph bugs: `properties rename` and
  `tags rename` under `--index` used to patch entries but never the link
  graph, so frontmatter link properties (`related`, `depends-on`) renamed
  across a vault left backlinks stale until a full `create-index` rebuild.
  A new `cargo run -p xtask -- check-mutation-journal` CI gate fails when a
  mutating command bypasses the journal.

### Added

- **`find --filenames0` and `--iteration <ID>` on `read`/`task`/`backlinks`
  (iter-238).** Two agent-CLI ergonomics follow-ups deliberately deferred out
  of iter-235. `find --filenames0` is the NUL-delimited sibling of
  `--filenames-only` (GNU `find -print0` precedent): each matching path is
  terminated by a NUL byte instead of a newline, so filenames containing
  newlines stay unambiguous and the output composes with `xargs -0`. Same
  rules as `--filenames-only`: no envelope/count/hints, zero results → empty
  output + exit 0, `--strict` still flips the exit code, and it conflicts
  with `--filenames-only`, `--jq`, `--count`, and an explicit
  `--format json`. Separately, every single-file command built on the shared
  input resolver — `read`, `backlinks`, and all three `task` actions — now
  accepts `--iteration <ID>`: like `set --iteration`, the ID must resolve to
  exactly one file (zero matches names the resolved globs; an ambiguous
  match lists the candidates), after which the command proceeds as if that
  file had been passed with `--file`.
- **pi package distribution: `pi install git:github.com/ractive/hyalo`**
  (iter-237). The pi extension and skills now live in a top-level
  `pi-package/` directory that is both a valid pi package (installable
  directly from the git repo, refreshable with `pi update --extensions`)
  and the single source of truth the hyalo binary embeds via `include_str!`.
  The old duplicate copies under `crates/hyalo-cli/templates/` are gone —
  package/binary drift is now impossible by construction. `hyalo init --pi`
  remains as the vendored installer of last resort and now prints a one-time
  hint pointing at the package install when it creates `.pi/` for the first
  time. `xtask check-bundled-skills` now also lints the pi-package skills,
  and `pi-extension-e2e.sh` type-checks the package copy.

- **Typed pi tools: `hyalo_find`, `hyalo_read`, `hyalo_set`, `hyalo_task`**
  (iter-236). The pi extension previously registered one generic `hyalo`
  tool where the model assembled CLI argv itself (`subcommand` + free-form
  `args[]`), and every dogfooding bug so far came from that surface:
  duplicate `--format text`, hallucinated flags, quoting of search terms.
  The four new typed tools take structured parameters instead — e.g.
  `hyalo_find` with `property: ["status=planned"]`, `tag`, `taskStatus`,
  `countOnly`, `limit`; `hyalo_set` with `file` + a single `K=V` property;
  `hyalo_task` with `mode: all|section|line`. All route through the same
  shared execution core as the generic tool (timeouts, signals, error
  rendering unchanged); the generic tool stays as the escape hatch for
  everything else. `pi-extension-e2e.sh` gains a layer forcing one call per
  typed tool, and `skill-hyalo-pi.md` teaches the typed tools first.

### Fixed

- **`--index` mutations (`set`/`append`/`remove`/`task toggle`/`task set`/
  `lint --fix`) now upsert a file the persisted index has never seen**
  (review of iter-225/226, BUG-1). Only `new --index` inserted a brand-new
  entry; every other mutating command's `MutationJournal` refresh patched
  an *existing* entry only, so mutating a file created after `create-index`
  (or present before the index existed) silently dropped it from the
  persisted index — `find --file <it> --index` reported 0 results even
  though the on-disk write succeeded. The journal now inserts a fresh entry
  (with link-graph registration) whenever the mutated file isn't already
  indexed.
- **`links fix --apply` no longer reads as a false "it worked" when it
  applied zero fixes** (review of iter-225/226, BUG-2). `applied` in the
  JSON envelope still means "apply mode was used" (iter-216 D-4, unchanged
  for backward compatibility — use the `applied_fixes`/`fixes` counts to
  tell whether anything actually landed), but the text-format summary line
  now reads `Applied: yes (0 fixes)` instead of a bare `Applied: yes` that
  looked identical to a run which actually rewrote links.
- **`--iteration <ID>` with a non-numeric value no longer reports "iteration
  ID is empty"** (review of iter-225/226, BUG-3). `--iteration abc` used to
  collapse into the same "empty" error as `--iteration ""`; the parser now
  distinguishes a genuinely empty ID from a non-empty one with no leading
  digit and reports `iteration ID 'abc' is not numeric` instead.
- **A heading containing a template expression is never reported as a dead
  anchor** (iter-215, carried over from iter-211's known limitation). A
  Liquid/Jinja heading (`## {% data variables.product.prodname_pro %}`)
  renders to `## GitHub Pro` and anchors as `#github-pro`, which hyalo cannot
  derive from the pre-render source — so every correct anchor into such a
  heading was reported broken on templated corpora like GitHub Docs. When
  neither the raw-text (DEC-060) nor the GitHub-slug (DEC-075) convention
  matches and *either* side carries a template marker (`{%`, `{{`, `${`) —
  the fragment, or any heading in the target file — the anchor is now treated
  as unknowable rather than broken. Same marker set as iter-207's
  `links fix` zone-skip, and it moves `find --broken-links`, `summary`'s
  `links.broken_anchors`, and the `links fix` anchor note together. A file
  with no templated heading still reports its dead anchors exactly as before.
  See DEC-099.
- **`append`/`remove <key>=<value>` touch only the appended or removed list
  item, not the whole list** (iter-219, dogfood NEW-5). Extends iter-214's
  minimal-diff guarantee inside a key's own span: appending one item to a
  block-style list now inserts one dash line; removing one item drops one
  dash line; flow-style lists (`tags: [a, b]`) stay flow. On a GitHub Docs
  corpus, one appended `redirect_from` entry used to change more than one
  line in 361 of 406 files (worst case 118 lines); it is now 0. Detected
  structurally (old/new list values differ by exactly one scalar item), so
  it applies uniformly to `append`, `remove --property k=v`, and
  `remove --tag t` with no CLI-layer changes. A replacement, reorder, or
  non-scalar item still falls back to the existing whole-key re-serialize.
  A list that itself doesn't fit the splicer's simple model — a `#`-comment
  interleaved between items, or a flow list with a trailing comment the
  tokenizer can't represent — now falls back with an explicit warning
  rather than a silent whole-key re-serialize that could drop the comment.
  See DEC-086.
- **Mixed line endings in a frontmatter block no longer silently drop or
  add `\r`s** (iter-219, dogfood NEW-7). A block that mixed `\r\n` and `\n`
  per line used to be re-expanded to a single style based only on the
  *opening* delimiter's ending, with no warning — untouched lines could
  lose their `\r` (or gain one they never had) on any unrelated `set`. This
  now routes through the same "full rewrite, explicit warning" fallback as
  every other unsupported shape (DEC-081); the file still ends up on one
  consistent line ending, but the warning names why. See DEC-087.
- **Frontmatter parser errors no longer leak Rust struct internals**
  (iter-219, dogfood NEW-8). A budget breach read `budget breached:
  ScalarBytes { total_scalar_bytes: 8205 }`; a duplicate key read `...,
  set DuplicateKeyPolicy in Options if acceptable` — both are now plain,
  actionable text naming what happened, with no leaked type names. See
  DEC-088.
- **A file whose last bytes are literally `---` (no trailing newline, no
  body) no longer gains one on write** (iter-219, dogfood NEW-16a).
- **`set` now notes when type inference silently retypes a previously
  string-valued property** (iter-219, dogfood NEW-16c), e.g. `code: '42'`
  (string) becoming `code: 42` (number) via `set --property code=42`. The
  write still proceeds — this is an advisory, like the existing date and
  enum/pattern advisories, not a new gate. See DEC-089.
- **`lint --fix` totals now describe the whole run, not the display-capped
  listing** (iter-218, dogfood NEW-6). `total_fixed`, `total_remaining`,
  and `total_conflicts` — in both the text footer and JSON — were
  accumulated inside the same per-file loop that builds the (`--limit`-capped)
  `files[]` array, so a low `--limit` silently under-reported them. On a
  GitHub Docs corpus, the default limit printed `conflicts 0` while
  `--limit 100000` showed `conflicts 12` — a user who dry-ran at the
  default limit saw a clean preview and then hit conflicts on `--apply`.
  Writes were always complete (the trees produced with and without
  `--limit 0` were byte-identical); only the report lied. The totals are
  now accumulated over every result before the display list is truncated,
  exactly like plain `lint`'s `total`/`errors`/`warnings` already were.
- **MD010, MD042, and (when enabled) MD052 report Unicode-scalar columns,
  not byte columns** (iter-218, dogfood NEW-11). Per DEC-073, `lint`
  columns are 1-based Unicode scalars; these three upstream
  `mdbook-lint-rulesets` rules compute their reported column from a byte
  offset (`str::find`, a byte cursor, or comrak's byte-indexed AST
  `sourcepos`) instead. A line reading `àéî<TAB>` reported column 7 for the
  tab (the byte offset) instead of column 4 (the scalar offset); an emoji
  line reported column 5 instead of 2. Every other default-on rule was
  audited against a multibyte fixture and found already correct (MD009,
  MD011, MD034 index a `Vec<char>`; the rest report either a constant
  column 1 or a length measured only over ASCII indentation/hashes, which
  cannot diverge from bytes). The fix converts the reported column for
  these three rule IDs after the fact, using the same line text upstream
  measured against — it does not touch `mdbook-lint-rulesets`.
- **The `links`/`links fix` `[writes]` fuzzy hint no longer promises an
  apply it will not perform** (iter-218, dogfood NEW-14). The hint counted
  every fuzzy candidate found, including ones below the confidence floor
  that `--apply-fuzzy` never writes — on GitHub Docs it read
  `# Review then apply 3253 lower-confidence fuzzy fixes [writes]`, and
  running that command verbatim applied 0 files because none of the 3,253
  cleared the floor. The hint now counts only candidates at or above the
  effective floor; when none clear it, the hint instead points at
  `--min-confidence` to review the below-floor candidates, and no longer
  claims to write anything.
- **`links auto --apply` no longer corrupts wrapped wikilinks, wrapped
  markdown links, multi-line HTML tags, or any bracketed text**
  (iter-217, dogfood NEW-1/NEW-2/NEW-10). The zone scan that decides what is
  safe to rewrite was line-scoped: a wikilink or markdown link whose target,
  label, or destination wrapped onto the next line was invisible to it, so a
  title mention inside the wrap got rewritten in place
  (`[[research/[[release-pipeline-unification]]|reusable`-style double
  brackets on a real own-KB file). CommonMark reference links
  (`[label][ref]`, `[ref][]`, shortcut `[ref]`, `![ref][ref]`, and
  `[ref]: url` definition lines) were not recognized at all and got
  rewritten wholesale, definition line included — 54 corruptions on
  vscode-docs, 8 on GitHub Docs. The scan is now block-scoped: a construct
  is inert across its whole span, including across a line break, but never
  across a blank line, heading, or fence, mirroring the block-scoping
  iter-207 already applies to cross-line code spans. Any well-formed
  `[...]` bracket span is now inert regardless of whether it resolves to a
  real link or reference — style-guide placeholders (`[ACCOUNT ROLE]`) and
  PR area tags (`[typescript-language-features]`) are not links either, but
  writing `[[target]]` touching or inside one produced the same nested
  bracket corruption. See DEC-084.
- **`links auto` never writes a link its own resolver would call
  ambiguous** (iter-217, dogfood NEW-4). Ambiguity was checked against the
  human-readable title, but the link is emitted as a filename stem — two
  files with distinct titles that happen to share a filename (e.g. two
  `pulls.md` in different directories) passed the title check yet both
  resolved to the same `[[pulls]]` stem. `links auto --apply` on a GitHub
  Docs corpus wrote 1,492 such links that `hyalo links` then reported as
  ambiguous. Ambiguity is now also checked against the emitted stem, so
  either file's title is skipped. See DEC-083.
- **`changelog add` continuation lines are hanging-indented, and a missing
  category subsection is created under `[Unreleased]`, not left implicit**
  (iter-220, dogfood CHG-1, found during iter-217). A multi-line `--message`
  without `--wrap` used to carry its second and later lines as a literal `\n`
  inside one logical entry, which wrote them out flush-left instead of
  indented under the bullet.
- **`find --fields links` inventory no longer depends on the broken/ok
  verdict** (iter-220, dogfood NEW-12). A resolvable same-file fragment
  link (`[a](#part-two)` where `#part-two` exists) used to be silently
  absent from the inventory while a broken one (`[b](#nope)`) was
  listed — completeness was backwards. Resolvable same-file anchors now
  always appear, with `broken_anchor` (present-when-true) carrying the
  verdict. Same-file entries are excluded from the `--orphan`/
  `--dead-end` outbound-edge count, since a same-file heading jump is
  not an edge to another file — a file whose only "link" is to its own
  heading still counts as an orphan.
- **`summary` and `links fix` no longer silently hide broken anchors**
  (iter-220, dogfood NEW-15). `summary` gains a distinct `links.broken_anchors`
  figure (omitted from JSON when zero) instead of folding anchors into
  `links.broken` or ignoring them — a vault whose only defect was a dead
  heading anchor used to report "Links: N total, 0 broken" while
  `find --broken-links` reported findings for it. `links fix` gains a
  one-line note ("N broken anchor(s) — see `find --broken-links`") when
  anchors are broken but targets are clean; the check only runs in that
  case, so it never adds a second resolution pass to a vault that already
  has broken targets.
- **bare `hyalo lint` no longer hides how much of the vault `[lint] ignore`
  dropped** (iter-220, dogfood UX-1). "68 files checked, no issues" on a
  386-file vault used to read as a clean bill of health even though 318
  files were silently config-ignored; the summary line now appends
  "(N ignored by [lint] ignore)" and the JSON envelope carries the same
  figure as `files_ignored`. A `--glob` whose matches are entirely
  ignored now prints the same exclusion notice the named-file form
  already did, instead of a silently vacuous "0 files checked, no
  issues" — a `--glob` matching a mix of ignored and kept files stays
  quiet (only the summary-line count), so a large partial sweep isn't
  buried in per-file noise.
- **four hint dead ends closed, and `fuzzy_fixes`/`backlinks` gained fields
  their JSON was missing** (iter-220, dogfood NEW-18). `views run <name>`
  now gets full hint parity with `find --view <name>` — previously zero
  hints where `find --view` emitted several. `lint-rules show <ID>` gets
  scoped-lint and toggle/revert hints instead of none. `task read --all`
  / `--section` on a file whose tasks are all already done now points at
  `find --task todo` instead of dead-ending. `fuzzy_fixes` entries carry
  `col` (1-based byte column of `old_target` on its line, omitted when
  stale) alongside `line` — asked for in iter-210's own task text but
  never delivered. `backlinks` JSON now reports `target` as the queried
  file's own canonical resolved path, identically on every entry, instead
  of each occurrence's own written spelling (which could disagree in
  `.md` presence or relative-path form for the exact same target file).
- **`hyalo read --format json` hints when `--frontmatter` was omitted, and
  nested mapping properties are typed `map` instead of `text`** (iter-220,
  dogfood UX-4). `read`'s JSON envelope silently drops the `frontmatter`
  key entirely without `--frontmatter`, so `--jq '.results.frontmatter.x'`
  read as `null` indistinguishably from "the property doesn't exist" —
  a hint now names the flag, and disappears once it was actually passed.
  Separately, `hyalo_core::frontmatter::infer_type` classified a nested
  YAML mapping (`versions: {fpt: ..., ghec: ...}`, GitHub Docs'
  `featuredLinks` shape) as `"text"`, the same label a plain string gets
  — `hyalo properties` and `find --fields properties-typed` now report
  `"map"` so a scalar property is distinguishable from a mapping one
  without inspecting the raw value. **Migration note:** a script keying on
  `type == "text"` to detect nested-mapping properties changes behavior —
  `infer_type` feeds only reporting output (`hyalo properties`, `find
  --fields properties-typed`), never schema validation, so `[schema]
  required`/type-checking is unaffected.
- **a hand-written `[n/m]` task count in a heading no longer doubles with
  the computed one in text output** (iter-220, dogfood NEW-16). `## Tasks
  [6/6]` with 1 of 2 checkboxes actually open used to render as `## Tasks
  [6/6] [1/2]` — the stale hand-written count appended right next to the
  correct computed one — in both `find --fields sections` and the
  single-section jq filter. The computed count now replaces a trailing
  `[n/m]`-shaped bracket group in the heading text rather than appending
  a second one; a heading with no task section (nothing to replace it
  with) keeps its own bracket text exactly as written.
- **path relocations (`FixStrategy::ShortestPath`) no longer count as
  `case_mismatches`** (iter-220, dogfood NEW-13). `[a](target.md)` resolving
  via bare-stem lookup to `sub/target.md` is a move, not a casing fix — it
  used to be counted and listed under "Case mismatches" alongside genuine
  `LinkCaseMismatch` fixes, understating what actually changed. `links fix`
  gains a separate `relocations`/`relocation_fixes` bucket and a matching
  "Relocations: N" text section; both buckets are still written by plain
  `--apply`, only the reporting changed. **Migration note:** `BrokenLinkReport`
  (hyalo-core) gains a new public field, `relocations: Vec<FixPlan>` — a
  source-breaking change for any external consumer constructing the struct
  by literal (all in-tree construction sites are updated).
- **`--dir` at a config root no longer prints a self-contradictory note, and
  `--dir <foreign-tree>` gets the same ancestor-config discovery `cd` already
  had** (iter-220, dogfood NEW-17). `--dir .` at the directory a `.hyalo.toml`
  itself lives in used to print `./.hyalo.toml does not apply, ./.hyalo.toml
  is in effect` — the identical literal path claiming both halves of one
  contradictory sentence — because the "does not apply" half was a hardcoded
  string rather than the actual shadowed config path; it now names the file
  from data and says "still in effect" when it really is the same file.
  Separately, `--dir <foreign-tree>` used to check only that exact directory
  for a `.hyalo.toml`, so a subdirectory of an otherwise-configured tree
  reported "no .hyalo.toml — built-in defaults" where `cd <foreign-tree> &&
  hyalo …` would have silently adopted the ancestor config; both entry points
  now resolve identically. See DEC-091 for the config-trust implication of
  extending ancestor discovery to `--dir`.
- **`hyalo config` no longer double-prints the malformed-config diagnostic,
  and the ancestor-adoption note already respected `-q`** (iter-220, dogfood
  UX-3). The malformed diagnostic used to appear once on stderr and again as
  the lead line of `hyalo config`'s own report body; the stderr copy is now
  skipped for that one command, since its report already carries it. Every
  other command still prints it on stderr, since their own output doesn't
  surface `malformed` at all. The ancestor-adoption note itself was verified
  to already honor `--quiet` — not a bug.
- `hyalo init --claude` no longer corrupts a hand-edited CLAUDE.md when a stray `<!-- hyalo:end -->` mention appears in prose before the real managed section (F3-2). It used to append a duplicate section instead of replacing the existing one, and a later `hyalo deinit` would then strip the original and orphan the duplicate.
- `hyalo lint`/`lint --fix` no longer abort the whole run on one invalid-UTF-8 or otherwise unreadable file, including a file whose `--fix` *write* fails partway through (a concurrent external edit, or a permission error) — that failure is now reported once for that file and the rest of the vault still gets fixed and its index entries still get patched (M-1).
- **`links fix`/`links auto` with `--index` no longer trust a stale snapshot silently** (dogfood v0.20.0, BUG-2 detection / DEC-241). An in-place edit of an indexed file (by an editor, Obsidian, or anything but hyalo's journaled write paths) used to be invisible to the link-discovery pass — `links fix --apply --apply-fuzzy --index` reported `broken: 0`, `applied: true`, file untouched, no warning. Every indexed entry's stored mtime is now compared against the file on disk before discovery; drifted files are rescanned from disk (persisted under `--apply`, in-memory only for a dry run) and a `warning: index is stale: N file(s) changed on disk since create-index` names the drift.
- **File-not-found hints no longer suggest `hyalo find --file <glob>`** (dogfood v0.20.0, UX-1). `--file` takes exact vault-relative paths, so the hinted command failed with the very error it was supposed to fix; the hint now says `find --glob <glob>`, which actually globs.
- **`--iteration <ID>` reaches zero-padded and archived files** (dogfood v0.20.0, UX-2). `--iteration 2` now also matches `iteration-02-*.md` (zero-padded spelling of a bare integer, at the template's `{n:0W}` width or minimum 2), and every variant falls back to a `**/` recursive glob so files archived under subdirectories of the template's directory (`iterations/done/iteration-02-links.md`) are addressable by natural key. `01`/`16b` forms and bare-`16`-does-not-match-`16b` semantics are unchanged.
- **`lint` text/JSON file listing is error-first, and MD011 stops flagging regex prose** (dogfood v0.20.0, UX-4). The per-file list is now ordered by error count, then violation count, so a 50-file display cap can never hide the run's errors behind warnings-only files; when errors are still truncated away, the show-all hint names how many (`… (4 errors hidden by the file cap)`). MD011 (no-reversed-links) suppresses matches whose parenthesized "text" contains regex metacharacters (`|`, `[`, `]`) *and* whose bracketed part does not look like a link destination — prose like `(3rd|[Tt]hird)[-_]` is a regex, not a broken link, and the autofix would have corrupted it, while genuine reversed links with bracketed text stay flagged.

### Changed

#### Breaking: `results` JSON key renames (iter-216)

Three `results` keys were renamed so that one name means one thing across
every command. Scripts reading the old names get `null` and must be updated;
`--format text` output is unchanged. The full survey behind these, including
the divergences deliberately left alone, is in
`hyalo-knowledgebase/research/results-json-shape-inventory.md`.

| Command | Old key | New key | Why |
| --- | --- | --- | --- |
| `lint` | `.results.total` | `.results.violations` | `.results.total` was the violation count while the envelope's `total` on the same payload was the file count — one name, two quantities, one document. |
| `links auto` | `.results.total` | `.results.matched` | It counted proposals (a numerator) and duplicated the length of `matches`; the denominator on that command is `scanned`. |
| `summary` | `.results.schema.files_with_issues` | `.results.schema.files_with_violations` | Same quantity `lint` already reported as `files_with_violations`. |

The rule those follow: the envelope owns `total`, and a `total` inside
`results` always means *the number of items the command considered*, never a
count of findings. `set`/`remove`/`append`/`properties rename`/`tags rename`
already complied (`total = modified + skipped`) and are unchanged.

#### Non-breaking: `results` keys added (iter-216)

- **`dry_run` is now always present on `lint` and `lint --fix` results**, and
  **newly present on `links auto` and `links fix`**. It used to be omitted
  when `false` on `lint` while `set`/`remove`/`append`/`mv`/renames emitted
  `dry_run: false`, so the same state read back as `null` from one command
  and `false` from another. On the two `links` commands there was no way to
  tell a preview from an `--apply` that had nothing to do: `applied` is
  `false` in both cases (`links auto --apply` over a vault with no linkable
  titles reports `applied: false`). Top-level `results` keys are always
  present; only per-item records inside arrays omit absent optional keys.
- **`skipped_count` added to `set`, `remove` and `append` results.**
  `properties rename` / `tags rename` already reported the skip set as a
  count only — their skip set is "every scanned file that lacks the
  property", which on a large vault is the whole vault and carries no
  information — so no single key answered "how many were skipped" across the
  mutation family. `skipped` (the list) is unchanged where it was already
  emitted.

- **`set`/`append --property a.b=x` now rejects the whole batch, instead of
  silently creating a literal `"a.b"` key, when a top-level `a` already
  exists as a mapping in a matched file's frontmatter** (iter-219, dogfood
  NEW-16b — behavior change: a write that previously succeeded now fails
  with an error). Scoped to `set` and `append` only — `remove` is
  unaffected, since removing a nonexistent literal dotted key was already a
  harmless no-op. hyalo does not support dotted path syntax for nested
  properties; only this specific collision is guarded — a dotted key with
  no colliding map is still unchanged (still a literal flat key). The
  guard runs as a pre-pass over every matched file before any file in the
  batch is written, so a collision found on file 7 of 50 does not leave
  files 1-6 already mutated. See DEC-089.
- **Frontmatter's internal scalar-content budget raised to match the
  documented 64 KiB limit** (iter-219, dogfood NEW-8), from an undocumented
  8192-byte parser default. A real GitHub Docs `admin/index.md` (7,961
  bytes of frontmatter) was about 40 redirect entries from becoming
  unreadable, well inside its own documented ceiling. See DEC-088.
- **`lint --fix` JSON reports `remaining_errors`/`remaining_warnings`
  instead of `errors`/`warnings`** (iter-218, dogfood NEW-6b — **BREAKING
  JSON shape change**). Both plain `lint` and `lint --fix` used the same
  `errors`/`warnings` keys for two different quantities: plain `lint`
  counts whole-run severity totals, `lint --fix` counted only what was
  left unfixed after the run. A script reading `.errors` off both command's
  output got answers to two different questions under one key name. The
  fix-mode shape now uses `remaining_errors`/`remaining_warnings`, which is
  what those counts have always actually meant; plain `lint`'s
  `errors`/`warnings` are unchanged. Update any script or `--jq` filter
  that reads `.errors`/`.warnings` from `lint --fix` JSON.
- **`links auto`'s heading skip now uses the same CommonMark ATX-heading
  rule as the rest of the zone scan** (iter-217, review follow-up), instead
  of "the trimmed line starts with `#`". A line like `#nothash` (no space
  after the `#`) is no longer treated as a heading and is scanned as
  ordinary body text; a real heading (`## Real Heading`, 1-6 hashes
  followed by a space or end of line, indent 0-3) is still skipped exactly
  as before.
- **`links auto --apply` emits an alias instead of silently rewriting
  rendered prose** (iter-217, dogfood NEW-3). A match whose surface text
  differs from the emitted target — including only by case (`Pulls` vs
  `pulls`) — now writes `[[target|matched_text]]` instead of substituting
  the bare target, which used to change what the page renders. On a GitHub
  Docs corpus, 22.2% of proposed insertions (7,968 of 35,860) altered
  rendered prose this way before this change. A plain `[[target]]` is still
  written when the matched text is byte-identical to the target. See
  DEC-082.
- **Frontmatter writes now touch only the keys they change** (iter-214,
  dogfood BUG-14). `set`, `remove`, `append`, `tags rename`,
  `properties rename`, `types apply` and `lint --fix` parsed the whole YAML
  block and re-serialized all of it, so a one-key change rewrote every line
  the serializer happened to format differently — 116 of 198 frontmatter lines
  on a real GitHub Docs `index.md` for a single added property, with long list
  items refolded into `>-` block scalars and `'` quote style flipped to `"`.
  Nothing was ever lost (the round trip was semantically exact), but the churn
  made hyalo unusable in repos where frontmatter is under code review. Writes
  now re-emit the original bytes of every unchanged key — preserving quote
  style, block scalars, flow collections, indentation, blank lines and
  comments — and serialize only what actually changed. Adding one property
  changes one line. Where a block cannot be mapped to per-key line spans
  (explicit `? key` syntax, top-level flow collections, invalid UTF-8, mixed
  line endings) the whole block is still rewritten, but never silently: a
  `warning:` on stderr names the file and the reason. See DEC-080/DEC-081.

### Added

- **`find --filenames-only`, `find`/`set --iteration <ID>`, and a self-healing
  vault-boundary error** (iter-235, agent-ergonomics review findings 1/2/3/5).
  These close the three highest-friction traps the ralph-loop port dogfood
  documented workarounds for instead of the tool providing the feature:
  - `find --filenames-only` prints one raw file path per line — no JSON,
    no envelope, no count, no hints — the `grep -l` projection of a find
    result set, usable in `sort`, `xargs`, and `while read` loops. Zero
    results → empty output, exit 0. Conflicts with `--jq`, `--count`, and an
    explicit `--format json` (mutually exclusive projections); `--strict`
    still flips the exit code, so `find --property status=planned
    --filenames-only --strict` is a CI gate that lists the offenders and
    fails.
  - `--iteration <ID>` resolves a sequence-keyed document by its natural
    key (`--iteration 206` → `iterations/iteration-206-*.md`) using the
    type schema's `filename_template` `{n}` slot, so iteration lookups no
    longer need a hand-built glob (`--property 'title~=206'` silently fails
    because the frontmatter title is `"Iteration 206 — …"`). Accepts a bare
    integer (`206`), zero-padded integer (`01`), or integer + letter suffix
    (`16b`); the ID is substituted verbatim, so `01` matches
    `iteration-01-*` (not `iteration-1-*`) and `16b` matches only
    `iteration-16b-*` (bare `16` does not match the suffixed file). On
    `find` this is a filter (returns every match); on `set` it selects the
    file to mutate and errors unless exactly one match is found, listing
    the candidates so the caller can disambiguate by suffix. `--where-*`
    still composes on `set --iteration` (filters within the selection).
  - The vault-boundary error now names the effective vault directory and
    offers a self-healing hint (`pass a path relative to <dir>, or cd to a
    parent of it`), so an agent that passed `/tmp/foo.md` learns the vault
    root and the fix in the same message instead of guessing another
    absolute path. The long-standing `resolves outside vault boundary`
    substring is preserved for existing assertions.

- **Every link in `find --broken-links` / `--fields links` now carries its
  source `line`** (iter-215, dogfood UX-6). The report listed every link of a
  matching file with no location, so finding the one that was actually broken
  meant grepping the file. Each entry now has a 1-based `line` — the same one
  `hyalo lint` (HYALO006) and `backlinks` report for that link — and text
  output prefixes each line with `line N:`. Links are listed in document
  order, so same-file anchors no longer trail the rest of the file's links.
  The line comes from the index (`IndexEntry::links` / `::self_anchors`
  already stored it), so no extra file read is involved and the `--index` and
  disk paths agree. `find --broken-links --jq '.results[] as $f | $f.links[]
  | select((.path == null and (.out_of_vault | not)) or .broken_anchor)
  | "\($f.file):\(.line) \(.target)"'` now produces a `file:line` list an
  editor can jump to.

- **`hyalo config --raw`, and a machine-readable malformed-config signal**
  (iter-213, dogfood UX-2). `hyalo config` exited 0 with a full set of
  built-in defaults when `.hyalo.toml` failed to parse, and said so only on
  stderr — a JSON consumer had no way to tell a configured vault from a
  broken one. `results.malformed` (always present) and `results.parse_error`
  now carry that state in the output itself, and the text rendering leads with
  it. The raw file text moved behind `--raw`: at several kilobytes on one JSON
  line it dominated `results` and buried the resolved values it sat next to.

- **`hyalo views run <view> <pattern>`** (iter-213, dogfood BUG-14). The
  subcommand's help promised equivalence with `find <pattern> --view <view>`
  while rejecting the positional outright. It now takes an optional BM25
  `PATTERN` with the same semantics — mutually exclusive with `-e`, and
  overriding a pattern saved in the view.

- **`views list`, `lint-rules list` and a clean `links` run now emit
  drill-down hints** (iter-210, dogfood UX-4). All three used to return an
  empty `hints` array, which made them navigation dead ends: the listing named
  a thing and never said what to do with it. `views list` offers to run (or
  delete) the first saved view, or to create one when the vault has none;
  `lint-rules list` focuses whichever rule the vault actually overrode and
  offers to inspect it, revert the override, or lint with just that rule; and
  `links` on a vault with nothing broken points at `links auto` and
  `find --orphan` — the two link questions a clean fix report does not answer.
  `links` also advertises "Apply N case-mismatch fixes", a repair plain
  `--apply` has always performed but never mentioned, because case mismatches
  are deliberately excluded from the `fixable` count.

- **A malformed `.hyalo.toml` names the command that fixes it** (iter-210,
  dogfood UX-5). `[types.note]` is the recurring mis-spelling of
  `[schema.types.note]`, and the raw serde error only listed `schema` among the
  accepted fields — enough to say you were wrong, not enough to say what to
  run. Unknown `types`, `rules`, `view` and `profiles` keys now carry a
  `fix:` line naming the real table *and* the `hyalo types set` /
  `hyalo lint-rules set` / `hyalo views set` command that writes it correctly.
  A malformed file with no recognised key gets no invented suggestion.

- **The `links auto` noisy-title note now fires on frequency, not just on an
  English word list** (iter-205). A title is flagged when it is a common
  English word **or** when it dominates the run — at least 25 proposed links
  and at least 2.5% of them, i.e. `max(25, ceil(total / 40))`. The wordlist
  alone missed the titles that actually matter: on a GitHub Docs slice a page
  titled `Workflows` produced 502 of 1,179 proposed links (43%) and was never
  mentioned, while the note named `limits` at 4%. The frequency trigger is
  arithmetic, so it is also the first version of this note a non-English vault
  ever sees — the word list is ASCII-only by construction. The note says which
  trigger fired per title, and quotes the share of the run for frequency hits.
  `[links.auto] warn_common_titles` and `--no-warn-common-titles` still govern
  both triggers; there are no configurable thresholds, and stdout stays
  byte-identical either way.

- **Directory link targets resolve to `<target>/index.md`** (iter-203). A link
  that names a directory now reaches that directory's index file: `/foo`, `foo`
  and `/foo/` all resolve to `foo/index.md`. This is the convention every
  static-site corpus publishes against (MDN, GitHub Docs, Docusaurus, Hugo),
  and without it such vaults read as almost entirely broken — MDN reported
  49,703 of 49,705 links broken and `backlinks` returned 0 for its most-linked
  pages. The rule lives in the single shared resolver, so every surface agrees:
  `find --broken-links`, `links`, the HYALO006 lint rule, `backlinks` (a link to
  `/foo` is a backlink of `foo/index.md`), anchors (`/foo#section` checks
  `foo/index.md`'s headings), and `mv` (renaming `foo/index.md` rewrites `/foo`
  to `/bar`, keeping the author's spelling). Precedence: a real file wins, so
  `foo` still resolves to `foo.md` when both `foo.md` and `foo/index.md` exist —
  write `foo/` to name the directory explicitly and flip that order.
- **`hyalo config` reports the effective `site_prefix` and its source**
  (iter-203). The prefix is usually *auto-derived* from the vault directory
  name, and it decides what a site-absolute `/foo` means — but `config` printed
  `site_prefix: (none)` whenever it had not been set explicitly, hiding the
  value actually in force. It now prints the resolved value with a
  `flag` / `config` / `derived` / `disabled` label, and the JSON envelope gains
  a `site_prefix_source` field.
- **`links auto --no-first-only`** (iter-198). The counter-flag to
  `--first-only`: it forces first-mention-only *off* for a single run, so a
  vault that persists `[links.auto] first_only = true` can still get a one-off
  all-mentions pass without editing `.hyalo.toml`. Previously the two sources
  were OR-ed, leaving the config key impossible to override per run. The flag
  conflicts with `--first-only` (passing both is a clap error, not a silent
  precedence puzzle) and is a no-op when `first_only` is not enabled.
- **`create-index --path` and `drop-index --output` flag aliases** (iter-195).
  The two commands name the *same* index file with different flags — `-o/--output`
  on `create-index`, `-p/--path` on `drop-index` — so each accepts the other's
  long spelling as a visible alias. Existing spellings and short flags are
  unchanged, and no second short flag was added.
- **Out-of-vault link targets are reported separately from broken ones**
  (iter-193). A relative link whose target normalizes to a path *above* the
  scanned vault root (`../../CONTRIBUTING.md`) cannot resolve to a scanned file
  no matter what — it is out of scope, not missing. `hyalo links` now reports
  it under `out_of_vault` / `out_of_vault_links`, `hyalo summary` counts it as
  `links.out_of_vault` (omitted when zero), `hyalo find` flags the link with
  `out_of_vault: true`, and `find --broken-links` no longer surfaces a file
  whose only unresolved link escapes the vault. Site-absolute targets
  (`/src/foo.md`) deliberately stay in `broken`: a vault that *is* the site root
  makes those genuine misses.
- **`list` / `summary` verb aliases across every subcommand group** (iter-192).
  `properties` and `tags` used `summary`; `types`, `views`, and `lint-rules`
  used `list`. Each group now accepts both spellings, so the verb learned in one
  group works in the rest: `hyalo tags list`, `hyalo properties list`,
  `hyalo types summary`, `hyalo views summary`, `hyalo lint-rules summary`.
- **Two new drift gates** (iter-192). `cargo run -p xtask --
  check-command-reference` fails when a clap subcommand has no COMMAND REFERENCE
  entry (it is wired into the Quality Gates workflow). An execution-based hint
  gate harvests every hint the CLI emits across a fixture vault and *runs* each
  one, failing on any the CLI rejects — the substring assertions it replaces
  could not distinguish a runnable command from a plausible-looking one.
- **Persistent `hyalo links auto` exclusions in `.hyalo.toml`** (iter-195a).
  A new `[links.auto]` section persists `exclude_titles`,
  `exclude_target_globs`, and `first_only`, so the incantation that makes
  auto-linking usable on a title-heavy vault no longer has to be retyped every
  run. The two lists are unioned with `--exclude-title` /
  `--exclude-target-glob` (flags extend the config, never replace it) and
  `--first-only` still forces first-mention-only for a single run. When config
  exclusions actually removed candidates the report says so: `config_excluded`
  in the JSON envelope (omitted when zero) and an
  `Excluded by [links.auto] config: N titles` line in text output.
  `hyalo config` reports the effective settings in both formats.
- **`links auto` names candidate titles that are common English words**
  (iter-197). Exclusions only help once the noise has been noticed, so a run
  whose proposed links come from pages titled with ordinary English words or
  generic doc filenames (`permissions`, `index`, `notes`, `README`) now prints
  one advisory `note:` on stderr naming those titles with their match counts and
  the `--exclude-title` flags that would skip them. Only titles that actually
  produced matches are named, so excluding them extinguishes the note. The
  stdout report is byte-identical whether or not the note fires; `-q`,
  `--no-warn-common-titles`, or `[links.auto] warn_common_titles = false`
  silence it. The word list is bundled in `hyalo-core::common_words` — no new
  dependency.
- **`links fix` warns when the effective `site_prefix` stripped 0 of N
  site-absolute links to a plausible vault path** (iter-220, dogfood NEW-9).
  On a real MDN checkout, the auto-derived single-segment prefix (`en-us`)
  case-insensitively stripped the `en-US/` segment from every
  `/en-US/docs/...` link but left a `docs/...` remainder that names no
  real top-level vault entry, silently turning a 110-second run into
  49,762 broken links with no signal the prefix itself was the problem.
  The warning names the effective prefix and points at `--site-prefix` /
  `.hyalo.toml` / `hyalo config`.
- **`find --strict` — a general CI-gate flag: exit 1 if a query returns any
  results, 0 if empty** (iter-220, dogfood UX-2). Composes with any filter
  combination; the primary motivation is `find --broken-links --strict` to
  fail a build on a dead heading anchor (before this, `find --broken-links`
  always exited 0 regardless of findings). See DEC-090 for why this was
  chosen over a new lint rule.

### Changed

- **`.hyalo.toml` is discovered from parent directories** (iter-213, dogfood
  UX-1). **Behaviour change.** The config was read from the working directory
  and nowhere else, so `cd docs && hyalo lint` silently re-rooted on built-in
  defaults: no schema, no `[lint] ignore`, no views, no `site_prefix` — the
  most common config accident, with no diagnostic, while the `--dir` spelling
  of the same mistake had warned loudly since iter-201. hyalo now walks up to
  the nearest ancestor `.hyalo.toml` and adopts it when its configured vault
  contains the working directory (nearest config wins; one that points
  elsewhere is not adopted). From a deeper subdirectory, where the vault is
  wider than the directory you are standing in, a stderr note says so and
  points at `--dir .`. The old "do not cd into the vault" scolding is gone
  with the problem that motivated it.

- **`links auto` reports excluded titles and excluded mentions separately**
  (iter-213, dogfood BUG-14). `config_excluded` counted *titles* while reading
  like a candidate count, so excluding one common title reported `1` next to
  hundreds of vanished proposals. It is now `config_excluded_titles` plus
  `config_excluded_mentions`, and the text summary states both.

- **Config-level `[changelog] path` refusals match the runtime ones**
  (iter-213, dogfood BUG-14). An absolute or escaping `[changelog] path` bailed
  with exit 2 and a single-line message, while the same refusal at write time
  exited 1 with the two-path form. Both are now exit 1 with the shared
  `<subject> resolves outside vault boundary: <resolved>` wording and the raw
  config value in the error's `path` field.

- **The config-integrity warning waits for `--dir` resolution** (iter-213,
  dogfood UX-5). An unusable `.hyalo.toml` was reported the moment it failed to
  parse, so `--dir other-vault` led with a warning about a file it then
  announced does not apply — the stale warning printed *before* the note
  contradicting it. It is now emitted once, for whichever config actually
  governs the run.

- **Hint commands put global flags last** (iter-213, dogfood UX-5). Several
  hint builders injected `--dir`/`--format` mid-command and appended
  `--glob`/file targets after them, so the same flags landed in different
  positions across the hints under one result set.

- **`--apply-fuzzy` now gates on a confidence floor, and the confidence means
  something** (iter-212, dogfood BUG-11, DEC-078). **Behaviour change for
  `--apply-fuzzy` users: fewer, better fixes.** The old confidence was a raw
  Jaro-Winkler score over filename stems, which rewards a shared prefix — so on
  GitHub Docs `/actions/reference/actions-limits` →
  `graphql/reference/actions.md` scored **0.9** (wrong document) while a
  genuine relocation whose basename matched byte-for-byte sat at the flat
  `BasenameFallback` constant **0.6**. The ordering was inverted relative to
  usefulness, and with no floor a bare `--apply-fuzzy` wrote every proposal.

  Confidence is now `0.7 · basename + 0.3 · directory`. The basename term is a
  soft token match (so `actions-limits` stops looking like `actions`, while the
  typo `configuraton` → `configuration` still scores 0.96); the directory term
  is three quarters *shared leading components*, so a relocation inside a
  section outranks a same-name substitution across sections. A target written
  with no directory asserts no location and is scored on its basename alone.
  The three dogfood proposals reorder to 0.87 / 0.504 / 0.533 — correct first.

  `--apply-fuzzy` writes only proposals at or above **0.8**, overridable with
  `--min-confidence <0.0-1.0>` or `[links] fuzzy_min_confidence` in
  `.hyalo.toml` (the flag wins; the config key never opts *in* to applying).
  `--min-confidence 0` restores the old accept-everything behaviour. Measured
  on the GitHub Docs corpus (3,710 files, 6,099 broken links) against the
  `redirect_from` metadata GitHub maintains as ground truth: the default floor
  applies **2,253** rewrites at **99.3% correct**, where the previous release
  applied **4,659** at **82.2%** — 804 links rewritten to a provably wrong
  document. Broken links still fall monotonically (6,099 → 3,846) and the run
  is idempotent.

  Reporting caught up too: the text report brackets each proposal with the
  strategy that produced it — `[basename-fallback 0.87]` vs
  `[fuzzy-match 0.91]`, where everything used to read `[fuzzy N]` — marks
  suppressed proposals `— below floor`, and names the floor. JSON gains
  `fuzzy_below_floor` plus `rule`/`below_floor` on each entry, and
  `fuzzy_min_confidence` is now always the effective number instead of `null`.
  `hyalo config` reports it as `links.fuzzy_min_confidence`.

- **A basename-only link repair is gated on whether the author wrote a
  directory, not on a leading slash** (iter-211, dogfood BUG-12, DEC-076).
  iteration 200 put `[x](/guides/actions)` → `reference/actions.md` behind
  `--apply-fuzzy` because discarding the written path is a guess — but left the
  byte-identical `[x](guides/actions)` as a plain-`--apply` fix. Same guess, two
  gates. The rule is now spelling-independent: any written directory component
  makes the repair a `BasenameFallback` that needs `--apply-fuzzy`, while a
  target written with no directory at all (`[[actions]]`, `[x](actions.md)`)
  resolves by the documented Obsidian short-form rule and stays a certain fix.
  On GitHub Docs this moves 4,659 repairs out of plain `--apply`. Relatedly, a
  link whose exact path fails but whose bare stem resolves elsewhere is no
  longer reported as `link-case-mismatch` at confidence 1.0 — a relocation
  dressed as a casing fix — but as `ShortestPath` at 0.95, and `links fix` text
  output now prints each fix's own rule code instead of a hard-coded label.

- **`FixStrategy::ShortFormStemMismatch` removed** (iter-211). The variant was
  `#[doc(hidden)]`, documented as reserved, and emitted by no code path since
  short-form stem casing was folded into `LinkCaseMismatch`.

- **`hyalo lint --rule-prefix <p>` fails when the prefix matches no rule**
  (iter-210, dogfood BUG-5). It used to print a warning and then lint
  *everything*: the empty rule filter fell through to "no filtering", so
  `--rule-prefix nope` ran every markdown rule and exited 0 — worse than the
  unknown-`--rule` case, which has failed loudly since iter-204. An unmatched
  prefix is now the same user error, naming the prefix and pointing at
  `hyalo lint-rules list`. Matching prefixes are unaffected and the match stays
  case-insensitive.

- **`links auto` JSON `col` counts characters, not bytes** (iter-210, dogfood
  BUG-13). `col` was 1-based but byte-indexed and undocumented, so a mention
  after an accented or CJK character reported a column no editor agrees with
  (character 9 was reported as 12). It now counts Unicode scalar values, the
  same convention `lint`'s `column` uses, and `links auto --help` says so. The
  byte offset the rewriter needs is kept internally and is no longer
  serialized; `--apply` output is byte-identical.

- **`links` text output accounts for every broken link** (iter-210, dogfood
  UX-4/UX-3). The summary block omitted the `fuzzy` bucket, so a GitHub Docs
  run reported `6098 broken` over `25 fixable + 1400 unfixable` and left 4,673
  links unexplained; JSON reconciled exactly. `Fuzzy matches` now has a count
  line beside `Fixable`/`Unfixable`, the previously JSON-only
  `unfixable_links` and `out_of_vault_links` are listed in text (capped at 20
  with an "and N more" footer), and the fuzzy per-fix listing — by far the
  longest section — moved to the end so the actionable buckets are no longer
  buried under thousands of fix lines. JSON lists stay uncapped.

- **The `links auto` noisy-title note is honest about truncation and spells
  titles the way your vault does** (iter-205, dogfood L-12/L-13). With more
  offenders than it lists, the note now says `showing the 5 noisiest of 7`
  instead of `+2 more`, and the suggested `--exclude-title` flags cover *every*
  offender rather than only the listed ones — pasting them back extinguishes
  the note in one round instead of two. Titles are displayed in their most
  frequent original casing (`README`, not `readme`), while matching and
  exclusion stay case-insensitive.

- **`hyalo lint --rule <id>` validates the id, and matches it
  case-insensitively** (iter-204, M-10). A typo'd rule id used to select
  nothing and exit 0 with "no issues found" — a CI gate that reads as green
  forever. An unknown id is now a user error naming the id, with the
  `hyalo lint-rules list` hint `lint-rules show` already gave; `--rule hyalo006`
  selects exactly what `--rule HYALO006` does. `--rule-prefix` matches
  case-insensitively too and warns on stderr when it selects no rule at all.
- **A write command whose single named file is unparseable exits 1**
  (iter-204, L-2). `hyalo set bad.md --property x=y` warned about the YAML
  parse error and then reported `0/0 modified (1 scanned)` at exit 0 —
  indistinguishable, to a script, from "already in the requested state". The
  same applies to `remove` and `append`. Batch runs (`--glob`, several
  `--file`s, whole-vault) are unchanged: there the other files genuinely were
  processed, and one bad note must not fail the run.
- **JSON match positions are 1-based in both axes** (iter-204, L-15).
  **Breaking for consumers that parse `col`.** `links auto` reported a 1-based
  `line` next to a 0-based `col`, so trusting one meant being off by one on the
  other. `col` now counts from 1, like `line` and like the `column` lint
  already emitted. Subtract 1 to recover the old byte offset.
- **The `--help` limit contract names only commands that actually cap**
  (iter-204, M-8). "Default output limits" listed all eight `total`-emitting
  commands, but `types list`, `views list` and `lint-rules list` reject
  `--limit` outright (they enumerate small fixed catalogs). Those two claims —
  "emits a total" and "caps at `default_limit`" — now come from two separate
  constants, each asserted against the real binary. Relatedly, bare
  `hyalo tags` and `hyalo properties` now accept the `--glob`/`--limit` flags
  COMMAND REFERENCE has always documented for them, instead of exiting 2.
- **`create-index`'s help documents the snapshot contract** (iter-204, M-6),
  and commands that load an index warn `index older than vault; results may be
  stale — re-run create-index` when the vault's top-level directory mtimes
  postdate the snapshot. Cheap by construction (one `read_dir`, no walk), so it
  misses in-place edits of existing notes and changes more than one level deep
  — it is a smoke alarm, not a guarantee.
- **mdbook-lint bumped to 0.16.0** (iter-196). `mdbook-lint-core` and
  `mdbook-lint-rulesets` move from 0.15.2 to 0.16.0, which ships the exact
  autofix-coordinate contract hyalo asked for upstream: `Fix` ranges are
  half-open, `Position` columns are 1-based Unicode scalars, CRLF is atomic,
  and `Position::to_byte_offset` is a checked conversion. Autofix output is
  strictly more accurate — fixes on lines containing multibyte characters
  (MD010 on a line with `✘` or a typographic apostrophe, for example) used to
  be dropped and are now applied. Violation counts are unchanged on every
  corpus tested. The upgrade also brings upstream's MD018 fix: a paragraph
  continuation line starting with an issue reference (`#472`) is no longer
  reported as a malformed ATX heading.

- **BREAKING: `--dir` no longer discards `.hyalo.toml`** (iter-201). `--dir`
  names a *vault*, not a config. When the path it resolves to is the one the
  working directory's `.hyalo.toml` already points at (`dir = "kb"` +
  `--dir kb`, the standard repo-root layout), that config now stays in effect;
  previously hyalo reloaded `.hyalo.toml` from the *vault* directory instead,
  found nothing there, and silently ran on built-in defaults — dropping the
  schema, saved views, `[lint] ignore`, per-rule severity overrides,
  `site_prefix` and the changelog path while printing "--dir is redundant".
  A `lint --strict` CI gate written that way went vacuously green. When `--dir`
  names a *different* tree the behaviour is unchanged (that tree's own
  `.hyalo.toml`, else defaults), but hyalo now says so on stderr, naming the
  file that took over. `hyalo config --dir <path>` reports the same resolution
  and no longer returns `config_path: null` while a config is in effect.
  **Migration:** runs that relied on `--dir` as a way to *ignore* the local
  config now honor it; pass `--dir` to a directory outside the configured vault,
  or move/rename the config, to get the old behaviour.
- **BREAKING: a malformed `.hyalo.toml` blocks mutating commands** (iter-201).
  One unknown key or type error anywhere in the file — including inside
  `[links.auto]` — made the whole config unusable and hyalo fell back to *all*
  defaults, `dir` included. `hyalo links auto --apply -q` could therefore
  rewrite a completely different tree than the one configured, with the warning
  suppressed. Writers (`set`, `remove`, `append`, `mv`, `new`, `task toggle`,
  `views set`, `links fix/auto --apply`, `lint --fix`, …) now exit 1 with the
  parse diagnostic and touch nothing; `--dry-run` invocations and `init`/`deinit`
  are unaffected. Read-only commands still run on defaults, but keep the `dir`
  value when it can be recovered from the file, and their config warning is no
  longer suppressed by `--quiet`.
- **Mutating drill-down hints are marked** (iter-201). `hyalo find` with two or
  more filters suggests saving the query as a view — a command that writes
  `.hyalo.toml` — and it was rendered in the same `-> hyalo …` list as read-only
  drill-downs. Writing hints now use a `=>` arrow with a trailing `[writes]`
  tag in text output and carry `"writes": true` in the JSON envelope, so "run
  the hints" is safe advice again. An e2e gate runs every hint the CLI marks
  read-only and fails if it changes a single byte of the vault or the config.

- **`summary --help` now documents the `-n` divergence** (iter-195). `-n` means
  `--recent` on `summary` but `--limit` on `find` and `backlinks`. The semantics
  are deliberately unchanged — `--recent` caps only the "recently modified" list,
  never the summary's stats, which always cover every scanned file — so the
  difference is now stated in a FLAG NOTE and in the flag's own help instead of
  being left to be discovered.
- **Read-only commands no longer write into the vault** (iter-193). With the
  default `[links] case_insensitive = "auto"`, hyalo detected the filesystem's
  case behaviour by creating and deleting a `.hyalo-case-probe-*` file in the
  vault root — uncached, at seven call sites, so `hyalo find --count` bumped the
  vault directory's mtime. Detection is now stat-only (an existing entry, or the
  vault directory itself, looked up under a case-flipped name) and memoized once
  per run. The write-based probe survives only for a vault that offers no
  candidate at all, and `create-index` sweeps orphaned probe files older than a
  minute. A side effect of the change: a **read-only vault that contains files
  now resolves case-insensitive links correctly** instead of silently falling
  back to case-sensitive resolution. See `docs/configuration.md`.
- **`mdbook-lint-core` / `mdbook-lint-rulesets` bumped 0.14 to 0.15** (iter-193).
  Upstream dropped its unused `mdbook` dependency, taking hyalo's normal
  dependency tree from 168 to 135 crates and a clean release build from 121 s to
  112 s. No source changes were required. The scoped MPL-2.0 license exception
  in `deny.toml` existed only for `mdbook` and is gone, so `cargo deny check`
  now passes with zero license exceptions.
- **`hyalo config --format json` now uses the standard envelope** (iter-192) —
  **breaking for JSON consumers**. The settings moved from the document root
  into `results`, and the config's own on/off switch is reported as
  `results.hints_enabled` rather than `hints`, which previously meant a boolean
  at the root and an array of drill-down commands everywhere else. `dir` stays
  hoisted to the root as well, so `hyalo config --format json | jq .dir` keeps
  working; every other field moves under `.results`. `config` also emits
  drill-down hints now, in both text and JSON.
- **`hyalo mv --apply` is rejected in single-file mode** (iter-192). Single-file
  `mv` writes immediately and batch `mv` defaults to dry-run; accepting
  `--apply` as a silent no-op in the first mode hid that asymmetry from anyone
  who learned the batch form first. It now errors with `single-file mv applies
  by default; use --dry-run to preview`. Batch `--apply` is unchanged.
- **One exit code and one wording for every vault-boundary refusal** (iter-202).
  `okf log` refused an escaping target with exit 2, the documented
  internal-error class, while the rest of the family used 1; `mv`,
  `create-index` and `drop-index` each phrased the same refusal differently.
  Every boundary refusal now exits 1 and reads `<subject> resolves outside vault
  boundary: <resolved target>`, with the path the user typed carried in the
  error's `path` field so both halves are visible. The
  `set`/`append`/`remove`/`task` family gained the resolved target it previously
  omitted.

### Removed

- **All downstream mdbook-lint autofix workarounds** (iter-196). With the
  0.16.0 coordinate contract in place, `hyalo-mdlint`'s translation layer no
  longer compensates for upstream range bugs: the `rule_uses_byte_columns`
  per-rule allowlist, the hand-rolled `line_col_to_byte` walk, the MD011
  inclusive-end `end += 1` guard, the MD034 Liquid-tag pull-back
  (`trim_md034_liquid`), and the `line_len + 1` replace-vs-insert heuristic
  are all deleted — `convert_fix` is now a straight checked translation.
  Each deletion is covered by a fixture that fails under 0.15.2 semantics.
  One documented exception survives: `md047_fix` still computes MD047's fix
  locally for **CRLF** bodies, because shipped 0.16.0 hard-codes `"\n"` for
  the missing-EOF-newline insertion. LF bodies use upstream's fix.

- **`hyalo_core::tasks::toggle_task` and `set_task_status`** (iter-191). These
  singular single-line mutators had zero callers in the CLI and lacked the
  `check_mtime` concurrent-modification guard their plural counterparts
  (`toggle_tasks`, `set_tasks_status`) already had. Their behavior is fully
  covered by calling the plural entry points with a one-element line slice.
  Breaking change for any external consumer of the `hyalo-core` library API.
- **Two zero-caller `pub` wrappers in `hyalo-cli`'s lint module** (iter-195).
  `commands::lint::lint_files` was a thin `FixMode::Off` wrapper over
  `lint_files_with_options` whose only caller was one unit test (dispatch has
  used `lint_files_extended` for several releases), and
  `commands::lint::prepend_file_result` was superseded by
  `inject_ext_file_result` when the extended lint path took over — it had no
  callers at all, not even a test. `FixMode`, `lint_files_with_options`, and
  `lint_files_extended` are untouched and still live.
- **`--iteration <ID>` natural-key addressing removed** (iter-242 / DEC-242, featureitis verdict on iter-235's flag). The flag — a third addressing mechanism next to `--file`/`--glob` whose zero-padding, letter-suffix, and `**/` recursion fallback rules were harder to predict than the glob it replaced — is gone from `find`, `set`, `read`, `task`, and `backlinks`, along with `hyalo-core::iteration_id` and the `FilenameTemplate` ID-glob helpers. Address sequence-keyed documents with a plain glob instead: `find --glob '**/iteration-02-*.md'` (the recursive form reaches zero-padded and archived files such as `iterations/done/iteration-02-links.md`); `read`/`set` take the same pattern via `--file` after a `find`, or the exact path. Help text and the bundled skills now teach this pattern directly.

### Fixed

- **The `create-index` help example runs verbatim** (iter-213, dogfood
  BUG-14). `hyalo create-index -o /tmp/my-index` was a documented example that
  the vault-boundary guard refuses, and the `--index-file` help ("absolute
  paths are used as-is") contradicted that guard. The examples now pass
  `--allow-outside-vault` where they need it, the boundary rule is stated on
  both flags, and the help documents the read-only-corpus workflow: build the
  snapshot outside the vault, then name it with `--index-file` on every query.

- **The index-mismatch warning names only what differs** (iter-213, dogfood
  UX-3). It printed the vault path twice even when the paths were identical
  and rendered prefixes through `{:?}`, so the reader got `Some("en-us")` and
  had to diff two long paths to find the one field that actually differed. It
  now reads `index does not match this run (site prefix: index 'en-us' vs run
  (none))`.

- **A fatal single-file parse failure is labelled `error`, not `warning`**
  (iter-213, dogfood UX-5). A write command naming exactly one unparseable
  file exits 1, but its diagnostic still carried the `warning:` prefix that
  means "the run continued".

- **`tags --limit` says how much it truncated** (iter-213, dogfood UX-5). The
  tag summary's own text renderer returned early and skipped the
  `showing N of M` footer that `properties --limit` prints.

- **`lint --fix` no longer prints a rule as both fixed and conflicted**
  (iter-213, dogfood UX-5). A rule with several violations in one file can
  have some fixes applied and one lose a range overlap; the two display lines
  read as a contradiction. The conflict line is suppressed when the rule
  already appears as fixed — the JSON keeps both, since `conflicts` is how a
  consumer learns a fix was skipped.

- **Heading anchors match the slug a renderer actually generates** (iter-211,
  dogfood BUG-8, DEC-075). `find --broken-links` compared a `#fragment`
  against the *raw* heading text only, so `[c](t.md#sub-section)` — the form
  GitHub, GitLab, MDN, Docusaurus and mdBook all emit for `### Sub Section` —
  was reported broken while `#Sub Section`, which no renderer produces, passed.
  Fragments now also match the GitHub slug (lowercased Unicode-aware,
  punctuation stripped, spaces to `-`, `-1`/`-2` suffixes for repeats); the
  Obsidian raw-text rule is unchanged and still accepted. On the GitHub Docs
  corpus 947 of 2,071 checkable anchors were false positives and now pass, while
  the 1,048 anchors that are genuinely absent from the source are still caught.
  Same-file fragments (`[b](#nope)`, `[[#nope]]`) are checked too — they carry
  no target path, so they were dropped at parse time and had never been
  validated at all. Rebuild the index (`hyalo create-index`) to pick up the new
  `self_anchors` data on the `--index` path.

- **HYALO006 line numbers no longer count the frontmatter twice** (iter-211,
  dogfood BUG-9). The broken-link rule scans the whole file, so its findings
  were already file-absolute, and the body-rule offset was then added on top: a
  link on line 5 of a file with 3 frontmatter lines was reported at line 8. The
  markdown rules and `backlinks` were right about the same file all along, which
  made the disagreement look like a corpus quirk. Exactness is now pinned by an
  e2e matrix over 0-, 3- and 5-line frontmatter.

- **One link occurrence is one backlink** (iter-211, dogfood BUG-10,
  DEC-077). With both `foo.md` and `foo/index.md` present, a single
  `[b](foo/)` was indexed under two keys and counted as a backlink of *both*
  files, while `links` simultaneously reported `ambiguous: 0`. Conversely
  `[b](/baz/)` resolved to `baz.md` in `find --broken-links` but was keyed as
  `baz/`, which `backlinks baz.md` can never probe, so the edge was invisible.
  Three fixes: a trailing slash survives relative-target normalization (so
  `[a](foo/)` and `[b](/foo/)` resolve alike), graph keys are stripped of the
  slash after the spelling has been recorded, and each occurrence registers
  exactly one key — the resolved one when resolution succeeded. `backlinks` and
  `find --broken-links` now agree on all eight dogfood spellings, verified
  corpus-wide against GitHub Docs.

- **Query strings and CommonMark link titles survive resolution and rewrites**
  (iter-211, dogfood BUG-12). `[x](/deep/page?x=1)` came back from `mv` as
  `[x](/deep/Page)` — the query was glued to the target so the rewrite span
  swallowed it; it is now split off like a `#fragment` and the span stops before
  the `?`. `[a](p.md "Title")` parsed the title as part of the destination, so
  the link resolved to nothing: reported broken, missing from `backlinks`, and
  unrewritable. Titles are now recognised (all three CommonMark forms, including
  one containing `)`), and the long-standing tolerance for unencoded spaces in
  destinations is kept.

- **`mv` preserves an extensionless spelling** (iter-211, dogfood BUG-12).
  `[f](foo/index)` came back as `[f](bar/index.md)` and `[[sub/b.md]]` as
  `[[archive/b]]` — a `.md` invented in one direction and dropped in the other.
  The suffix is a spelling choice orthogonal to the path shape, so it is now
  mirrored from the original for every form. All ten spellings the dogfood
  enumerated round-trip unchanged in style.

- **`lint` JSON counters describe the whole run, not the truncated file list**
  (iter-210, dogfood BUG-6, found independently by two testers). `results.total`
  and `rules_fired` were computed inside the display-capped loops while
  `errors`/`warnings` came from the full pass, so on MDN's `web/api` the JSON
  reported `total: 1358` against 14,248 errors+warnings — a tenfold
  disagreement inside one object — and `rules_fired: 7` where 8 rules fired.
  `files_truncated` was derived from `files_checked > limit` rather than from
  actual list truncation, so it was `true` on every clean vault over 50 files.
  All three now come from the pre-cap pass; the text renderer, which was always
  right, is unchanged.

- **The directory hint no longer doubles the path separator, and
  `did you mean X.md?` is only offered when `X.md` exists** (iter-210, dogfood
  BUG-13). `hyalo lint sub/` answered `--glob 'sub//*'`; pasting that back
  matched nothing and exited 0, so a directory that was never linted reported
  as a clean vault. `hyalo lint nosuchdir/` answered
  `did you mean nosuchdir/.md?` — a path that can never exist — because the
  candidate was built by string concatenation without ever checking disk. Both
  hints are now produced from the trimmed path and gated on the candidate
  resolving, and the executed-hint e2e gate runs *error* hints too, not just
  the ones attached to a successful envelope.

- **`find --file <missing>` reports not-found at exit 1** (iter-210, dogfood
  BUG-13 / L-7). It used to scope the query to a path no entry could match and
  print "No results" at exit 0 — indistinguishable from a query that genuinely
  matched nothing, and the opposite of what `lint` and `read` do with the same
  argument. All three now emit the same `file not found` envelope, including
  the `--glob '<dir>/*'` hint for a directory argument. A path already present
  in a `--index-file` snapshot is still accepted without touching disk, and an
  existing file that simply matches no filter is still a clean empty run.

- **Hints stop repeating a long absolute snapshot-index path** (iter-210,
  dogfood UX-5). Every derived `find` hint has to carry `--index-file <path>`
  or it would silently rescan the vault and answer a different question, so the
  same path appeared four or five times in one hint block. The path now renders
  in its working-directory-relative form whenever that is shorter, which keeps
  each hint runnable verbatim while removing most of the bulk — an index
  almost always lives inside the project it indexes.

- **`links auto --apply` no longer injects wikilinks into code spans after an
  unmatched backtick** (iter-207, dogfood BUG-1). One stray backtick — the
  common `` press <kbd>`</kbd> `` shape — used to pair with the *opening*
  backtick of a code span in a later paragraph, shifting every subsequent
  delimiter by one so real code read as prose: `` `git blame` `` became
  `` `[[git]] blame` `` and `` `settings.json` `` became
  `` `[[settings]].[[json]]` ``. Code spans are inline constructs, so the
  multi-line lookahead now stops at the end of the current block (blank line,
  heading, or fence) and an unmatched run stays literal, as CommonMark
  prescribes. Measured on the dogfood corpora: 9 silent corruptions on GitHub
  Docs, 8 on vscode-docs and 3 in hyalo's own knowledgebase, now 0.

- **`links auto --apply` no longer writes inside Liquid/Jinja expressions**
  (iter-207, dogfood BUG-2). 3,328 of 11,141 insertions on a GitHub Docs copy
  (30%) landed inside `{% … %}` or `{{ … }}`, turning
  `{% data variables.copilot.x %}` into a broken variable reference. Both
  marker forms are now inert zones for candidate matching; an unterminated
  marker makes the rest of the line inert rather than corrupting it.

- **`links auto --apply` no longer writes inside raw HTML tags** (iter-207,
  dogfood BUG-3). 128 insertions on vscode-docs and 5 on GitHub Docs landed in
  tag spans, breaking `src`/`href` paths, anchor names and class hooks
  (`<img src="[[net]].png" alt="[[actions]]">`). Tag spans — open and close
  tags, HTML comments, processing instructions, and quoted attribute values —
  are now inert. Text *between* tags stays linkable: in `<div>prose</div>`
  only the two tags are off limits.

- **`links fix` never rewrites a templated destination** (iter-207, dogfood
  BUG-4). A target containing `{%`, `{{` or `${` is a template expression, not
  a path. hyalo used to read `{% ifversion ghes %}/admin{% endif %}/guides` as
  literal text, fuzzy-match the remainder at 0.95 and rewrite it — silently
  dropping the version conditional. The round-trip guard could not catch this
  because the rewritten target genuinely resolves; the corruption is semantic.
  25 such rewrites were offered on the full GitHub Docs corpus, now 0. They
  are reported in their own `templated` / `templated_links` bucket rather than
  dropped.

- **An in-vault symlink no longer shadows the real file it points at**
  (iter-207, dogfood BUG-7, regression from iter-202). Canonical dedup kept
  whichever spelling sorted first, so `alias-target.md -> target.md` removed
  `target.md` from enumeration: a link fixable at `[fuzzy 0.966]` reported as
  `Unfixable: 1`, and fixes that did land were attributed to the alias name.
  The non-symlink spelling is now the group's representative; the first-in-sort
  fallback applies only when every spelling is a symlink.

- **Site-prefix stripping is case-insensitive** (iter-204). The prefix that
  decides what a site-absolute `/foo` means is usually auto-derived from the
  vault directory's name, and directory casing rarely matches the casing
  authors write: an MDN checkout in `en-us/` publishes `/en-US/docs/...`, so
  every site-absolute link stayed unresolved. Auto-derivation still yields a
  single path segment — a multi-segment prefix such as `en-US/docs` must be
  passed explicitly — and `hyalo config` now says so next to the derived value.
- **`backlinks` counts a case-mismatched wikilink once** (iter-204, L-1).
  `[[NOTE]]` pointing at `note.md` was registered under both the written and
  the canonical key, which land in the same case-folded bucket, so one link
  reported as two. `find --fields links` and `summary` were always right.
  `mv` still rewrites such links.
- **`mv` refuses to clobber a dangling symlink at the destination** (iter-204,
  L-4). The collision guard used `exists()`, which follows symlinks, so a
  symlink with a missing target read as "nothing there" and the rename
  destroyed it silently. Both single-file and batch mode now see the directory
  entry and stop, naming the broken symlink.
- **`drop-index` says "index file not found" when that is the problem**
  (iter-204, L-7). A nonexistent path *inside* the vault was reported as a
  boundary-check failure with an `--allow-outside-vault` hint that could not
  possibly help. The boundary is still enforced — a missing path under an
  out-of-vault directory is refused as before.
- **`create-index -o <custom>`'s drop hint carries `--path <custom>`**
  (iter-204, L-9). The hint said bare `drop-index`, which targets
  `<vault>/.hyalo-index` — a different file. The pairing is now gate-checked
  end-to-end by executing the emitted hint.
- **`read` errors honor the piped-JSON default** (iter-204, L-5). `read`
  prints raw markdown rather than JSON when piped, by design — but its errors
  followed that override too, so a scripted failure came back as prose. Every
  `read` error path (missing file, directory target, bad `--lines`, the
  `--count` rejection, `--section` miss) now emits the standard JSON envelope
  when piped, and still reads as text on a terminal or under
  `--format text`.
- **`find -e` regex errors use the standard error envelope** (iter-204, L-6),
  quote the pattern as typed, and no longer leak hyalo's internal `(?i)`
  prefix — which also shifted the regex engine's caret four columns off the
  real offending character.
- **`read --section` misses list the five closest headings, not all of them**
  (iter-204, UX-2). The error dumped every heading on one line — about 4 KB on
  this project's own decision log, punitive in a terminal and pure token burn
  for an agent, with the heading you meant buried in the middle. Candidates are
  now ranked by closeness to what was asked for, capped at five, and followed
  by a count. Files with five or fewer headings still list them all.
- **`mv` no longer injects a site prefix the author never wrote** (iter-203,
  L-11). Rewriting a site-absolute link always prepended the effective prefix,
  so a working `[x](/notes/old.md)` in a vault whose prefix was auto-derived
  from its directory name became `[x](/my-vault/notes/renamed.md)` — a link that
  resolves nowhere. The prefix is now re-emitted only when the original
  spelling carried it.
- **`.hyalo.toml` is parsed once per run, not twice** (iter-201). The help
  banner re-loaded the config the CLI had already resolved, so every invocation
  paid a second parse and any config warning ended with a spurious
  "1 additional identical warning(s) suppressed" line. The deprecation check for
  the kebab-case `required-sections` key also re-parsed the whole file; it now
  reads the already-parsed `[schema]` value.
- **`links fix --apply` no longer breaks the links it rewrites** (iter-200).
  The writer emitted the *vault-relative* path of the repaired target, but the
  resolver reads a bare markdown destination as relative to the source file's
  own directory and a `/…` destination as site-absolute — so on any corpus of
  site-absolute or nested relative links, every single rewrite produced an
  unresolvable target and the same "fix" was proposed forever. On a GitHub Docs
  copy this modified 1,097 files while the broken count went *up*. Repairs are
  now emitted in the form the link was written in (site-absolute stays
  site-absolute, relative is computed from the source directory, `.md`
  presence preserved), and an auto-derived `site_prefix` is never injected into
  a link that did not already carry it. A new round-trip guard refuses any fix
  whose emitted target would not resolve, reporting it under `unfixable`
  instead of writing it — so a future writer/resolver asymmetry can only cost a
  count, never a link.
- **Site-absolute link targets now reach the exact and case-insensitive fix
  strategies** (iter-200). The leading `/` (and any configured `site_prefix`)
  is stripped before matching, so `[x](/how-tos/Moved-Page)` is repaired as the
  case fix it is instead of falling through to a basename guess.
- **A same-named file elsewhere in the vault is no longer treated as a certain
  match for a site-absolute link** (iter-200). `[GitHub Actions](/actions)` was
  "resolving" to `graphql/reference/actions.md` — labelled `LinkCaseMismatch`
  at confidence 1.0 and written by a plain `--apply` — even when
  `actions/index.md` existed. Such a match now reports as the new
  `BasenameFallback` strategy at a reduced confidence and is grouped with fuzzy
  matches, so it is only written under `--apply-fuzzy` / `--min-confidence`.
  Bare and relative targets keep the existing shortest-path treatment.
- **`links auto --apply` no longer writes wikilinks into URLs or link labels**
  (iter-200). Candidate matching skipped inline code and whole-label matches
  but not markdown link *destinations*, bare URLs in prose, autolinks, or a
  substring of an existing link's label — so a page titled `net` rewrote
  `[x](https://pkg.go.dev/x/actions.summerwind.net/v1)` into
  `…summerwind.[[net]]/v1`, destroying working URLs. All four contexts are now
  inert, whether the link's destination is internal or external.

- **`hyalo --help` no longer contradicts itself about which commands emit
  `total`** (iter-192). Four help sections and the `--count` runtime error each
  carried a separately hand-written list, and none of the five matched the
  binary — `lint`, `types list`, `views list`, and `lint-rules list` all emit a
  `total` and accept `--count` while being listed as if they did not. All five
  call sites now render one `LIST_COMMANDS` constant, and an e2e test parses the
  list back out of the binary and verifies each named command really does emit a
  `total`.
- **Eight commands were missing from COMMAND REFERENCE** (iter-192):
  `changelog`, `config`, `lint`, `lint-rules`, `madr`, `new`, `okf`, and
  `types`. The reference also claimed `--format json|text` with a fixed
  `default: json` (it is `json|text|github`, defaulting to text on a terminal
  and json when piped) and omitted `--index-file`.
- **`hyalo tags summary` suggested a command that does not run** (iter-192). Its
  show-all hint emitted `hyalo tags --limit 0`; `--limit` belongs to `tags
  summary`, not `tags`. The equivalent `properties summary` hint had the same
  defect and is fixed too.
- **`hyalo config --jq` silently ignored the filter** (iter-192), printing the
  whole object as though no filter had been passed.
- **Mutating a symlinked note no longer destroys the symlink** (iter-191,
  DEC-062) — **user-visible behaviour change**. Every command that writes a
  file (`set`, `remove`, `append`, `task`, `lint --fix`, `mv`, `okf`,
  `changelog`, managed regions, link rewrites) funnels through one atomic-write
  helper, and that helper replaced the *symlink* with a regular file holding
  the new content. The aliasing relationship silently disappeared and the real
  target kept the stale content — silent data loss for any vault that aliases
  notes. The write now **follows** the link (bounded at 32 hops; a loop is
  refused) and replaces the target, leaving the symlink a symlink, which is
  what Obsidian does. A symlink whose target escapes the vault is still refused
  with `file resolves outside vault boundary` — following is never an escape
  hatch — and the boundary is re-checked against the *resolved* destination at
  every call site that knows the vault dir.
- **Atomic writes are now actually durable** (iter-191, DEC-063). The temp file
  is `sync_all`ed before the rename and, on Unix, the parent directory is
  `sync_all`ed after it. Previously a crash right after the rename could leave
  the new directory entry pointing at unwritten blocks — i.e. an empty or
  truncated note replacing good content. Unlike the snapshot index, which
  detects a torn read and falls back to a disk scan, user markdown has no
  recovery path.
- **`lint --fix` can no longer panic on a malformed fix range** (iter-191). A
  fix whose span was inverted (`start > end`) or landed mid-UTF-8-character was
  only checked against the body length, so either case sliced a `str` at an
  invalid offset and aborted the run. Such fixes are now reported as conflicts
  and skipped, leaving the file untouched.
- **Three unchecked writers now refuse to write outside the vault** (iter-202).
  `madr toc` built `<adr-dir>/README.md` straight from its positional argument,
  so `../outside` — or an in-vault ADR directory that is a symlink pointing
  out — created or rewrote a README anywhere on disk at exit 0.
  `changelog add`/`release` followed a `CHANGELOG.md` symlinked out of the
  vault; an intentional out-of-vault changelog configured via
  `[changelog] path` stays allowed, only the silent symlink hop is refused. `new --file` validated its
  path lexically but never resolved it, so an in-vault `outdir -> ../outside`
  symlink was a file-creation primitive. All three now resolve the destination
  before touching the filesystem and refuse at exit 1 with nothing written
  outside the vault, in dry-run as well as apply.
- **An in-vault symlink and its target count as one file** (iter-202). The vault
  walker enumerated both directory entries, so a whole-vault writer processed
  the same file twice. `links fix --apply` rewrote the note once per spelling
  and the second write saw the mtime the first had just changed — "modified by
  another process", exit 1 in CI, even though the fix had landed. The same
  double-count inflated `find --count`, `summary` totals and glob-write
  counters, and printed the out-of-vault-symlink skip warning once per internal
  walk. Enumeration is now deduplicated by canonical path — first spelling in
  sort order wins, stably across runs — and each skip is warned once per run.

### Security

- BREAKING: a project-local `.hyalo.toml` whose `dir` is absolute or nets above the config directory now refuses every command (iter-221, H-1). Previously honored verbatim, so a cloned repo could point the vault root at its own parent or an absolute path and every downstream boundary gate then defended containment against that attacker-chosen root instead of the real one. An in-bounds relative `dir` (including a bounded `sub/../kb` round-trip) and an explicit `--dir` are unaffected; `hyalo config` still reports the problem (`dir_out_of_bounds`) instead of being refused. See DEC-092.
- BREAKING (behavior, not API): `create-index`/`--index-file`-backed commands (`set`, `mv`, `task toggle`, and every other command that patches a snapshot index in place) now refuse to save the index through a symlink whose target resolves outside the vault, instead of following it unconditionally (L-1). A first cut of this fix routed the write through `fs_util`'s *unguarded* `atomic_write`, which follows a symlink chain with no boundary check at all — a `.hyalo-index` symlinked to a file outside the vault let any of those commands silently overwrite that outside file with index bytes. Now routed through `atomic_write_within`, which follows an in-vault symlink target per DEC-062 (replace the target, keep the symlink) but refuses — with a clear error, leaving the outside file untouched — when the resolved target is outside the vault. An index destination that is itself outside the vault and not a symlink (e.g. an explicit `create-index --output /elsewhere --allow-outside-vault`) is unaffected.
- Bound `--jq` filter evaluation: a filter now gets a 3-second wall-clock deadline (evaluated on a worker thread), a 1,000,000-value output-count cap, and a per-value byte-length check before any copy, alongside the existing 10 MiB output-size cap. Previously an infinitely-recursing filter with no output (e.g. `def f: f; f`) hung the process forever, a filter that built a huge intermediate before emitting anything (e.g. `[range(3e8)] | length`) used 4.8 GB RSS to print one number, and a single huge value (e.g. `"x" * 2000000000`) used ~4.0 GB RSS by duplicating itself into a second buffer just to be measured against the cap. All three now error cleanly within the deadline, the last one at roughly half the peak memory. Not covered: a filter whose recursion overflows the native stack (`def f: [f]; f`) still aborts the process — no user-space hook can intercept that.
- Windows drive-relative paths (`C:foo`) and NTFS Alternate Data Stream markers (`a.md:stream`) are now rejected by both the snapshot-index path validation and the `--file` resolution boundary check (M-2). Windows-only; a colon is an ordinary filename character elsewhere.
- The case-insensitivity filesystem probe no longer creates and deletes a transient file inside the vault directory (ADVISORY-c). It now writes to the system temp directory when verified to be on the same filesystem as the vault (falling back to the vault only when that can't be confirmed), so it no longer pings file watchers or flickers in `git status`.
- Bumped `anyhow` to 1.0.104, resolving RUSTSEC-2026-0190 (unsoundness in `Error::downcast_mut`, patched >= 1.0.103); hyalo does not call that API, but the fix was a clean drop-in upgrade.

### Fixed

- **`hyalo find <CJK query>` no longer silently returns zero results** (iter-223, F-2). BM25 tokenization split text only on non-alphanumeric characters, so a whitespace-free Chinese/Japanese/Korean run collapsed into one giant, unmatchable token — the module claimed "Unicode-aware" tokenization but was Unicode-*safe*, not CJK-*aware*. Scriptio-continua runs (CJK ideographs, Hiragana/Katakana, Hangul) are now additionally tokenized as overlapping character bigrams; queries tokenize the same way, so a CJK substring query matches. A no-separator mixed run (e.g. `日本語Docker入門`, ordinary CJK technical writing) is segmented at every scriptio-continua/non-scriptio-continua boundary before tokenizing, so an embedded Latin term stays a whole, findable word instead of being fragmented into bigrams alongside the surrounding CJK text. The ASCII fast path is unchanged (verified byte-identical output and no measurable perf regression on a 3,710-file English corpus), and a persisted BM25 index built before this fix is detected via a new `tokenizer_version` tag and transparently falls back to a live re-tokenization rather than continuing to serve unmatchable results — no forced rebuild required, though `create-index` restores full pre-tokenized speed. Known limitation: a single-character CJK query cannot match a longer run (bigram-only indexing has no unigram entries for a multi-character run). See DEC-095.
- **`--jq` runtime errors no longer embed the entire input value** (iter-223, F-3). jaq's runtime error `Display` includes the JSON value it failed on (e.g. the whole `.results` array for a filter like `.results | .file` applied to an array), which on a large vault could dump megabytes of vault content into the error envelope — a content-disclosure vector for any consumer that logs errors. Both the jq runtime-error and input-conversion-error paths are now truncated to ~200 characters with a `…` suffix and name the failing filter.
- **Mixed-type `--sort property:<KEY>` no longer looks like nonsense with no explanation** (iter-223, F-4). The comparator's deliberate total order (comparing raw JSON text across types, e.g. `priority: "10"` vs `priority: 9`) is unchanged, but a stderr warning now names the property and its distinct types when a sort key mixes JSON types across the result set, so a frontmatter typo (a quoted number) is visible instead of silently producing a type-grouped-but-not-numerically-sorted order.
- **`resolve_file` no longer claims an in-vault `../file.md` "resolves outside vault boundary"** (iter-223, F3-4). From a vault subdirectory, `hyalo read ../broken.md` — naming a file squarely inside the vault — used to report the same message as a genuine escape. The lexical no-`..` policy is unchanged (a `..` component is still always rejected, regardless of where it would land), but it now gets its own honest message ("path contains '..' and is rejected... paths must be vault-relative without '..' components") instead of reusing the (here, false) "outside vault" wording. See DEC-097.
- **File-not-found, empty-path, and the F3-4 no-`..` errors now carry an actionable `hint`** (iter-223, F3-5). Previously these three common failures returned bare `{"error": ..., "path": ...}` with no guidance — an agent-driven CLI whose whole UX is drill-down hints had no hint on its most common error paths. `file not found` now hints that paths are vault-relative and suggests `hyalo find --file <glob>`; an empty path hints at shell-quoting; the F3-4 message hints at the vault-relative form to use instead.

### Changed

- **`task toggle --section` (and `task read`/`task set --section`) refuse an ambiguous multi-heading match instead of silently applying to all of them** (iter-223, F-1). A `--section` selector that matches more than one distinct heading instance (e.g. two `## Tasks` headings under different ADRs) used to toggle every task under every match with no warning; it now errors, naming the matched heading line numbers, and suggests `--line` to disambiguate. A single matching heading (with any number of tasks under it) is unaffected. `find --section` deliberately keeps its existing union behavior on the same kind of ambiguous match (a vault-wide read has no single "the" match to refuse against) but now warns once per run with the affected file count, and both `find --help` and DEC-094 state the asymmetry explicitly. See DEC-094.
- **BREAKING: schema property constraints now reject unknown keys, and `type = "number"` supports `minimum`/`maximum`** (iter-223, F3-3). `RawPropertyConstraint` (the deserializer for `[schema.types.*.properties.*]` blocks) now denies unknown TOML keys — a typo like `patterns` (for `pattern`) or any other unsupported key is now a config error instead of being silently dropped, consistent with the module's existing "misconfigured TOML surfaces as an error" stance for every other field combination. On a malformed `[schema]` block this is a loud warning plus schema validation disabled for that run (not a hard command failure), matching how every other schema misconfiguration has always been handled — but a vault carrying a stray key in a property constraint block does lose enforcement until the key is fixed, hence BREAKING. Also new: number properties can declare inclusive `minimum`/`maximum` bounds (`type = "number"`, `minimum = 1`, `maximum = 5`), enforced by `hyalo lint`. A malformed `[schema]` block is now also a visible lint-result violation on a `.hyalo.toml` pseudo-file (not just a stderr warning): `hyalo lint --strict` exits non-zero and names the bad key, and a non-strict run always shows the diagnostic in `results` rather than reporting a false-clean "no issues" while validation is silently disabled. See DEC-096.

- **`links fix` fuzzy-matching performance (iter-206).** Profiled the ~12.7 s GitHub Docs `links` runtime flagged since the v0.21.0-pre dogfood: ~87% of samples sat in `strsim::jaro_winkler` inside `LinkMatcher::find_match` — the fuzzy-candidacy gate was re-run over the whole vault for every broken link (O(broken × vault)). `LinkMatcher` now precomputes per-file stems at build time and caches the threshold-gated candidate shortlist per *distinct* target stem, so the full-vault gate runs once per distinct stem; per-link self-link filtering and `candidate_confidence` ranking are unchanged and results are byte-identical (two new `matcher_shortlist_cache_*` unit tests pin this). Measured on a 961-file GitHub-Docs-shaped scratch corpus: `links fix` dry-run 4.84 s → 0.95 s with realistically repeated broken targets (the MDN no-prefix stress shape), 4.67 s → 3.14 s in the adversarial all-unique-target case; `links auto` is unaffected (0.05 s). A `cargo run -p hyalo-core --example links-perf <dir>` phase-timing harness is committed for future re-attribution, and `xtask bench-scale` passes with an order of magnitude of headroom (median 1.43 s vs 15 s budget).

### Added

- **Test-quality hardening: concurrency/crash coverage for the atomic-write path, `cargo-fuzz` targets for the four parser surfaces, typed JSON assertions in the e2e suite, Windows M-2 e2e coverage, and an on-demand scale gate** (iter-224, closing the six T-1..T-6 gaps from `reviews/deep-analysis-2-2026-08-23.md`). No user-facing behavior change; this is test/tooling infrastructure only. Highlights:
  - A new e2e suite asserts `fs_util::atomic_write` never leaves a file torn under either contention (N concurrent `hyalo set` processes racing the same file, verified by a concurrent reader) or interruption (`SIGKILL` mid-write, polled to land during the temp-file window rather than a guessed sleep).
  - `fuzz/` (cargo-fuzz, its own workspace — excluded from `cargo build/test/clippy --workspace`, run manually alongside Miri per the project's existing security-review workflow, never in CI) adds targets for frontmatter parse+splice, the streaming scanner, wikilink/link-label extraction, and the MessagePack snapshot-index loader, each seeded from real knowledgebase fixtures.
  - The `links`/`lint`/`find` e2e suites now deserialize CLI JSON output into the real typed output structs (`hyalo_core::types::*`, `commands::lint::ExtLintOutput`/`ExtLintFixOutput`) via a shared `typed_results` helper, instead of indexing an untyped `serde_json::Value` by string key. For the `find`/`lint` suites the target structs are the production types, so a production field rename is a compile error across those assertions; the `links fix` conversions deserialize into a test-local mirror struct (its production output is built ad hoc via `json!()`, with no production struct to reuse), so a rename there fails at test runtime as a missing-field error rather than at compile time. `Deserialize` was added to the relevant production structs alongside their existing `Serialize`.
  - `crates/hyalo-core/src/discovery.rs` and `case_index.rs` gain Windows-gated (`#[cfg(windows)]`) e2e and unit tests for the M-2 drive-relative-path/NTFS-ADS rejection (both the `--file` resolution gate and the snapshot-loader gate) and a real-NTFS case-insensitivity behavior pin, plus a cross-platform lexical shape table for the underlying string check.
  - `cargo run -p xtask -- bench-scale` times `find`/`links fix` against a deterministic ~14,000-file synthetic vault against a fixed budget — an on-demand regression gate, not a per-PR CI check (see DEC-098 for the cadence/budget rationale).

## [0.20.0] - 2026-07-19

### Added

- **Broken-anchor detection in `find --broken-links`** (iter-190, L-21): links
  now carry their `#fragment` (heading anchor) through parsing and resolution.
  `find --broken-links` reports a **broken anchor** — a link whose target file
  exists but whose `#heading` does not — as a category distinct from a broken
  target. In JSON, an anchored link gains `fragment` and (when the heading is
  missing) `broken_anchor: true`; text output renders `"Foo#Real" → "Foo.md"`
  and marks a missing heading as `(broken anchor)`. The two categories are
  never both reported on one link (a broken target skips the anchor check), and
  broken anchors do **not** inflate `links fix`'s `broken` / `fixable` counts or
  its "Apply N fixes" hint — `links fix` stays target-only, letting anchor
  semantics soak one release behind `find` before any lint/CI gate consumes
  them. Anchor matching is exact and case-insensitive (Obsidian convention:
  `[[Foo#tasks]]` matches `## Tasks`), decodes percent-encoded markdown
  fragments (`foo.md#my%20heading`) for comparison only (the written form is
  preserved), and skips `^block-id` refs (block ids are not indexed). Validation
  reads headings from the already-materialized index/scan sections — **zero
  extra file reads** on the `--index` path, and no per-file re-read on disk scan.
  `mv` and `links fix` preserve fragments byte-exact (the rewrite span stops
  before `#`).

  The `Link` wire-shape gained an additive `fragment: Option<String>` field
  (`#[serde(default)]`), serialized into `.hyalo-index` entries and the
  persisted link graph. The field is **backward compatible**: existing
  `.hyalo-index` snapshots load unchanged (fragments read as `None`, so no false
  anchor reports from stale entries). To pick up fragment data for anchor
  validation on the `--index` path, **rebuild the index** with `hyalo
  create-index` after upgrading.

- **`HYALO006` / `broken-link` lint rule** (iter-188): `hyalo lint` now flags
  wikilinks and markdown links that point at a vault file which does not exist.
  Enabled and `warn` by default; `hyalo lint --strict` promotes it to an error
  so CI can gate broken links. The vault-wide resolution context (case/stem
  index) is built **once** per invocation — from the `.hyalo-index` snapshot
  when `--index` is active, else a single vault walk — and shared across
  workers, so the rule adds no per-file graph rebuild. Respects
  `[lint.rules.HYALO006]` enable/severity overrides, `--rule HYALO006` /
  `--rule-prefix HYALO`, and `--files-from` (resolution stays vault-wide even
  when the linted set is scoped, so a scoped file linking to an
  unscoped-but-existing file does not false-positive).

- **Honest partial-failure envelopes for link write paths** (iter-187): when a
  file write fails mid-batch, `hyalo links fix --apply`, `hyalo links auto
  --apply`, and batch `hyalo mv --apply` now emit a complete JSON envelope
  rather than aborting with a bare error. `links fix --apply` gains `failed` /
  `failed_fixes` buckets (each with the per-file error string); `links auto
  --apply` gains `files_applied` / `files_skipped` / `files_failed` counts plus
  a per-file `apply_outcomes` list (applied/skipped/failed with reason, so skips
  that previously only went to stderr are now in the envelope). Any partial
  failure yields a non-zero exit code. Files written before the failure are
  reported as applied, never silently kept and unreported.

### Fixed

- **`find --orphan` / `--dead-end` / `--fields backlinks` count
  case-insensitive inbound links** (L-6 tail): `[[foo]]` pointing at `Foo.md`
  now counts as inbound in `find`, matching the `backlinks` command and
  `summary` (all three route through the same case-insensitive graph lookup).
  Previously `find --orphan` could list a file that `summary` and `backlinks`
  agreed had inbound links.
- **Percent-encoded markdown link destinations now resolve** (iter-188, L-23):
  `[x](my%20dest.md)` previously never resolved (the `%20` was compared
  literally against the on-disk filename `my dest.md`), so `find --broken-links`
  false-positived and `backlinks "my dest.md"` missed the linker. The path
  portion is now percent-decoded during resolution and in the link graph, so
  encoded and angle-bracket (`[x](<my dest.md>)`) forms resolve to the same
  file. Malformed (`%2`, `%zz`) or non-UTF-8 (`%FF`) escapes keep the literal
  text — a filename with a stray `%` still resolves as written. Rewrite keeps
  the destination as-authored (the `%20` form is preserved on `mv`).
- **Batch `mv --apply` no longer leaves dangling links after a rolled-back
  rename** (PR #221 review): when a mid-batch write failure rolled back file
  renames, a "self-rewrite" plan — one whose rewritten content was written to
  a file's own new (renamed) location, e.g. a moved file's outbound link
  rewrite — was previously left in place even though its rename was undone,
  stranding the file at its old path with content referencing the (now
  reverted) new layout. Such plans are now identified by `path` coinciding
  with one of the batch's own rename destinations, and their pre-batch
  content is restored alongside the rename rollback. Plans on files outside
  the rename set (pure external linker files) still keep the original
  DEC-056 behavior of being kept and honestly reported.
- **`hyalo links fix --apply` no longer aborts the whole batch on a per-file
  I/O error** (PR #221 review): a `stat`/read failure for one source file
  (e.g. deleted between detection and apply) now lands that file's fixes in
  the `failed`/`failed_fixes` envelope and the remaining files in the batch
  still get their fixes applied, instead of propagating the error and losing
  all progress.
- **`hyalo summary` orphan/dead-end counts are now case-insensitive** (iter-189,
  L-6): inbound-link membership for orphan/dead-end classification went through
  a case-*sensitive* target-set check, so on a case-insensitively-written vault a
  file `Foo.md` linked only as `[[foo]]` was miscounted as an orphan even though
  `hyalo backlinks Foo.md` found the linker. Inbound membership now uses the
  `lower_index`-backed `backlinks_ci` lookup (the same one `backlinks` uses), so
  such a file is correctly reported as a dead-end and orphan counts agree with
  the backlink view. Outbound membership is unchanged (on-disk paths compared
  against on-disk paths — no case divergence). Note: `find --orphan` /
  `find --dead-end` still compute inbound via the case-sensitive `backlinks`
  path; aligning them is a documented follow-up (see iter-189) so this release
  ships exactly one observable orphan/dead-end change.

### Changed

- **`hyalo links fix` dry-run validates plans against on-disk text** (iter-187):
  dry-run now runs the identical plan-building phase as `--apply`, so its
  `unapplied` / `unapplied_fixes` fields report exactly the fixes `--apply`
  would refuse (stale index / concurrent edit) instead of always being empty.
  The "Apply N fixes" hint count now discounts would-be-stale fixes so it
  matches what `--apply` actually writes.

- **Classify-side link resolution collapsed onto the shared resolver** (iter-189,
  refactor only): the `links fix` verdict logic (`resolve_and_classify_link`,
  `classify_link`, `classify_short_form_wikilink`, plus the `LinkResolution` /
  `StemIndex` types) moved out of `link_fix.rs` into `discovery.rs` as
  `classify_link_from_source` — the Classify-mode sibling of the Exists-mode
  `resolve_link_from_source`. Both now route their kind-dependent normalization
  through one private `normalize_link_target` helper, so Exists ("does this link
  resolve?") and Classify ("full fix-policy verdict") can no longer drift on the
  wikilink/markdown/site-absolute/bare-basename branching. The test-only
  `detect_broken_links(&[FileLinks])` twin was deleted and its five unit tests
  ported onto `detect_broken_links_from_index`. No user-visible behavior change
  (locked by e2e capturing the `broken`/`case_mismatches`/`ambiguous` buckets).
- **Shared link-existence resolver entry point** (iter-188, task 0): the
  "does this link exist?" resolution that `find --broken-links` /
  `find --orphan` / `find --dead-end` and the new HYALO006 rule both need is now
  a single `discovery::resolve_link_from_source` function. It owns the
  kind-dependent normalization (wikilink vault-relative, markdown
  site-absolute / path-qualified / bare-basename) and the final
  `resolve_target` call, so `find/mod.rs` no longer inlines that branching and
  the lint rule does not reimplement it.
- **Unified link write path** (iter-187): `auto_link` now builds
  `RewritePlan`s and writes through the shared `execute_plans_partial`
  machinery instead of a hand-rolled line splitter (removed
  `split_lines_preserving_endings`), keeping its stronger full-content TOCTOU
  guard. Batch `mv` reports which link rewrites were durably applied before a
  mid-batch abort (DEC-056: completed content writes on untouched linker files
  are not rolled back; the renames are, along with the content of any
  self-rewrite plan whose path coincided with a rename destination).

## [0.19.0] - 2026-07-19

### Added

- **`hyalo lint` accepts multiple positional files** (iter-179): `hyalo lint
  a.md b.md` lints every listed file, matching `--files-from` semantics; the
  positional `FILE` argument is now repeatable.
- **`hyalo mv` accepts a positional destination** (iter-181): `hyalo mv old.md
  new.md` is now an alias for `hyalo mv old.md --to new.md`, matching the
  positional-file ergonomics of the other mutation commands. The positional
  `DEST` requires the positional source and is mutually exclusive with `--to`.
- **`hyalo changelog add --wrap <cols>`** (iter-181): word-wrap a long entry
  message on word boundaries into a hanging-indented bullet (2-space
  continuation indent), for 80-column changelogs.
- **`hyalo set` emits an advisory note for enum/pattern violations** (iter-181):
  setting a value the type's schema would reject (an out-of-enum value or one
  failing a `pattern`) now surfaces the same kind of non-blocking `note:` that
  date violations already get. The write still proceeds — `hyalo lint` (or
  `set --validate`) remains the enforcement gate.

### Changed

- **`--format github` is deterministic and truncation-honest** (iter-186):
  annotations are now emitted sorted by `(path, line, rule)`, so which findings
  GitHub keeps under its per-step annotation cap is stable across runs. hyalo
  still emits every workflow command, but GitHub registers at most 10 `error` +
  10 `warning` annotations per step — when a run exceeds either cap hyalo now
  appends a `::notice::` stating the true totals so the truncation is visible
  (quiet when both are under the cap). The exit-code contract is unchanged.
  The project's own CI (`.github/workflows/ci.yml`) is split accordingly: a
  diff-aware `lint-kb` job lints only a PR's changed files
  (`git diff origin/$BASE...HEAD | hyalo lint --files-from -`) so the annotation
  budget is spent on the PR's own findings, plus a full-vault `lint-kb-full` job
  on push to main to catch cross-file regressions the diff-aware check can't
  see.
- **Exit-code contract: flag-conflict user errors exit 1, not 2** (iter-181):
  combining `--jq` with `--format text`, `--count` with `--jq`, `--count` on a
  non-list command, and `--format github` on a non-lint command now exit `1`
  (user error) instead of `2` (which the help reserves for internal errors).
- **`hyalo set` JSON response echoes the coerced value** (iter-181): the
  `value` field now reflects the parsed YAML value written to frontmatter (e.g.
  a list for `--property 'x=[a, b]'`, a number for `x=3`) rather than the raw
  input string.
- **`hyalo new` omits schema-violating placeholders** (iter-181): when a
  required pattern/length-constrained string has no valid default, the scaffold
  no longer emits an invalid `TBD` value (e.g. `branch: TBD` against
  `^iter-\d+[a-z]*/`); the key is omitted for the user to fill, and a later
  `hyalo lint` flags it as missing-required.

### Fixed

- **Angle-bracket link destinations are parsed correctly** (L-A1): a
  CommonMark-valid markdown link destination like `[text](<my dest.md>)` —
  which hyalo's own generator has emitted since iter-176 — is no longer
  stored with literal `<>` characters. `find --broken-links` no longer
  false-positives on these links, and `backlinks` now resolves them.
- **Escaped brackets in link text no longer drop the link** (L-A2): a label
  containing an escaped bracket, e.g. `[Contains \[test\] brackets](dest.md)`,
  no longer terminates the label scan early and silently discards the whole
  link from `--fields links` and `backlinks` output.
- **Property-regex parse errors surface the engine detail** (iter-181): an
  invalid `--property 'title~=('` filter now reports the regex engine's own
  message (with caret/position) as the error `cause`, the way `find -e` does,
  instead of dropping it.
- **Hints preserve the vault context and active filters** (iter-180,
  BUG-7/BUG-8): the `create-index` hint after a slow or large-vault command now
  carries the explicit `--dir` (running it verbatim indexes the right vault, not
  the default one) and drops the dangling `…queries:` colon. Derived `find`
  hints now compose with the active graph/title filters — a "Show all N" or
  "Narrow by tag" hint on a `--orphan` / `--broken-links` / `--dead-end` query
  keeps that filter (and any `--index-file`), so the suggested command
  reproduces the same scoped set instead of widening to the whole vault. When
  the shown results were a truncated page, the misleading per-tag/per-status
  count is dropped rather than presenting a page-local number the command would
  not return.
- **`summary` schema counter is honest** (iter-180, BUG-9): the schema
  error/warning tally now applies `[lint] ignore` globs and the hint is
  relabelled `Schema: N errors, M warnings` pointing at `hyalo lint --rule
  SCHEMA` — the exact command that reproduces those counts (plain `hyalo lint`
  also runs MD body rules, so its totals never matched the schema-only counter).
  The stale "Show all N files with issues" hint is suppressed after a `lint
  --fix` apply, where the pre-fix count no longer holds.
- **Fewer false-positive did-you-mean suggestions** (iter-180): `summary` no
  longer flags enumerated numeric-suffix values (`hero-6` vs `hero-4`, `v2` vs
  `v3`) as possible typos of one another.
- **Site-URL diagnostic for absolute-link vaults** (iter-180): when nearly every
  link in a link-heavy vault is unresolvable (e.g. an MDN-style copy where
  49,933/49,935 links are absolute site URLs), `summary` now suggests setting
  `--site-prefix` instead of offering `links fix` on tens of thousands of
  unfixable links.
- **Lint respects fenced code and inline code spans** (iter-179, BUG-5):
  HYALO001 (bare-checkbox) and HYALO002 (completed-tasks) no longer fire on a
  `[]` or literal `- [ ]` that appears inside a ``` / ~~~ fenced code block or a
  `` `…` `` inline code span — documenting checkbox/array syntax in prose is no
  longer flagged. This removed the entire HYALO001 false-positive class on real
  MDN prose.
- **Body lint reports file-absolute line numbers** (iter-179, BUG-6): a body
  rule's `line N` now counts from the top of the file (offset past frontmatter),
  matching the raw file; the HYALO001 message no longer embeds a redundant,
  body-relative line number that disagreed with it.
- **Per-violation severity matches the counts** (iter-179, BUG-17): each lint
  line is labelled with its own `error`/`warn` severity, so a folded `SCHEMA`
  group that mixes the two no longer renders `error` lines that the summary
  tallies as warnings.
- **Lint message polish** (iter-179): summary and hint counts pluralize
  correctly (`1 error, 0 warnings`); the `--files-from` missing/outside-vault
  hints use singular/plural grammar; the HYALO005 frontmatter-parse message no
  longer double-prefixes (`could not parse frontmatter: failed to parse YAML
  frontmatter: …` → single prefix); MD034's autolink fix no longer swallows a
  trailing Liquid tag (`{% … %}` / `{{ … }}`) into `<…>`; and `changelog add`
  into an existing empty `### Category` keeps a blank line after the heading.
- **Frontmatter wikilink anchors survive `mv` and `links fix`** (iter-178,
  L-2/L-7): an anchored frontmatter link such as `related: - "[[decision-log#DEC-041]]"`
  is now rewritten with its `#anchor` preserved when the target moves, and
  `links fix` repairs keep the anchor instead of dropping it. Both paths route
  through a single shared `rewrite_frontmatter_wikilink_text` helper so they
  stay symmetric.
- **Self-referencing frontmatter links survive a rename** (iter-178, L-1): the
  moved file's own frontmatter self-links (e.g. `related: - "[[a]]"` when moving
  `a.md`) are now rewritten to the new path in both single-file and batch `mv`,
  instead of being left as a dangling reference.
- **`mv --index` refreshes the source link graph** (iter-178, L-5): after a
  move with `--index`, a subsequent `backlinks --index` query reflects the
  rewritten source outbound links (the index now refreshes both the entry and
  its graph edges, matching the live scan).
- **`links fix` no longer desyncs on a `%%` inside a code fence** (iter-178,
  L-8): a literal `%%` line inside a fenced code block is treated as code, not
  an Obsidian comment delimiter, so links after the block are still repaired.
- **Fuzzy link matcher accepts a lone valid candidate** (iter-178, L-9): a
  single fuzzy candidate scoring just above the threshold is no longer wrongly
  rejected as an ambiguous "tie" against the threshold value itself.
- **Case-only rename works on case-insensitive filesystems** (iter-178, L-14):
  `hyalo mv a.md --to A.md` on macOS/Windows no longer fails with "target file
  already exists" when the source and destination resolve to the same inode.

## [0.18.0] - 2026-07-18

### Added

- **OKF (Open Knowledge Format) support** (iters 163–166): `datetime-tz`
  property type (timezone-aware timestamps, disjoint from naive `datetime`);
  `[schema] exempt` glob list binding reserved files (`index.md`, `log.md`) to
  no schema, honored by lint and validate-on-write; `hyalo init --profile okf`
  writes an OKF-ready `.hyalo.toml` and installs a bundled `okf` skill with
  `--claude`; `hyalo okf index` / `hyalo okf log` reserved-file generators
  (deterministic, managed-region-aware, dry-run by default with a non-zero
  exit on drift); `hyalo lint --profile okf` applies the same profile fragment
  as an ephemeral overlay and adds six warn-level OKF conformance rules
  (reserved-file structure, citations, augmentation guards). Bundle-root
  absolute links are supported via `site_prefix = ""`.
- **Composable profiles**: profiles are declarative TOML fragments deep-merged
  (upserted) into `.hyalo.toml` — multiple `init --profile <p>` runs coexist
  in one vault, re-running a profile is idempotent, and user-authored keys the
  profile doesn't own are never touched (iter-164).
- **`madr` profile** (iter-167): `adr` schema type (status lifecycle,
  supersede pattern, MADR 3.x `deciders` alias, required
  Context/Options/Decision sections) bound to `docs/decisions/**` via the new
  generic `[[schema.bind]]` path-bound schemas (ordered, first-match-wins
  globs, wired into lint, validate-on-write, and fix); `{n:04}` zero-padded
  filename-template tokens; `MADR-SUPERSEDE-RESOLVE` and
  `MADR-DUPLICATE-NUMBER` advisory lints; `hyalo madr toc` dashboard
  generator.
- **`skills` profile** (iter-168): validates Agent Skills `<name>/SKILL.md`
  files (path-bound `skill` schema, name↔dirname coupling, reserved names,
  description and body-length budgets) with three advisory rules; generic
  string `min_length`/`max_length` schema constraints.
- **`changelog` profile** (iter-169): validates `CHANGELOG.md` against the
  Keep a Changelog 1.1.0 grammar (heading sequence, semver-descending
  versions, category subsections, footer link references) through a new
  reusable declarative heading-grammar engine, with eight `CHANGELOG-*` lint
  rules; `hyalo changelog release <X.Y.Z>` rotates `[Unreleased]` into a dated
  version section and `hyalo changelog add` appends categorized entries —
  both dry-run by default. This file is maintained with them.
- **`hyalo lint --format github`** (iter-170): emits one GitHub Actions
  workflow command per violation (`::error` / `::warning` with repo-root
  relative paths and spec-compliant escaping) so findings render as inline PR
  annotations; lint-only; output caps are lifted so no annotation is silently
  dropped.
- **Companion GitHub Action**
  [`ractive/setup-hyalo`](https://github.com/ractive/setup-hyalo) (iter-171):
  installs the prebuilt hyalo binary on any runner (checksum-verified against
  the release `SHA256SUMS`, tool-cached); the README documents the two-step
  PR-check recipe and the `claude-code-action` agent recipe.
- **`[scan] include` config** (iter-175): glob allow-list re-admitting
  specific hidden subtrees (e.g. `.claude/skills/**`) to the vault walker for
  every command (`.git` stays hard-excluded). The skills profile ships
  `include = [".claude/skills/**"]` so `**/SKILL.md` bindings reach the
  canonical Claude Code skill location.
- **`[changelog] path` config** (iter-175): point the `changelog` commands at
  a file outside the vault — e.g. the repo-root `CHANGELOG.md` when `dir` is
  a docs subdirectory — with a path-escape guard.
- **xtask `check-bundled-skills` CI gate** (iter-175): every bundled skill
  template is linted as installed under the skills profile, so a bundled
  skill can never ship violating its own schema again.
- **`okf index` / `madr toc` non-destructive adopt** (iter-173): a marker-less
  `index.md`/`README.md` is now *adopted* — its entire hand-written body is
  preserved and the managed region is appended after it (dry-run reports
  `adopt (preserving N existing lines)`). The old overwrite behavior is opt-in
  via a new `--replace` flag. On case-insensitive filesystems an existing
  `INDEX.md` is recognized as the reserved file and adopted by its on-disk
  casing.
- **`[okf] ignore` config**: vault-relative globs (`_template/**`,
  `test/fixture-vault/**`) the OKF generators skip, independent of
  `[lint] ignore`.
- **`HYALO005` / `frontmatter-parse-error` lint rule** (iter-174): a file whose
  frontmatter cannot be parsed (invalid YAML, duplicate keys, oversized scalar)
  is now reported as an error-severity lint violation under a stable rule id and
  still counts toward `files_checked`, so it appears in text/json/github output
  and fails CI. Listed in `hyalo lint-rules list`; severity configurable via
  `[lint.rules.HYALO005]` but never silently downgraded by a profile.
- **Skip-summary in text & github** (iter-174): when `--files-from` drops input
  paths, `--format text` prints a `note: N input paths missing, M non-markdown
  skipped` line (stderr) and `--format github` emits the same as a `::notice::`,
  matching the counters JSON already exposes. An explicitly named `--file`
  excluded by `[lint] ignore` prints a notice instead of a silent `0 files
  checked`.
- **Distinguishable `--fix --dry-run --format github`** (iter-174):
  would-be-fixed violations render as `::notice` with a `[fixable]` title prefix
  and the summary becomes `N fixable, M remaining`, so a dry-run preview is no
  longer byte-identical to a plain lint run.

### Changed

- **BREAKING (CI): unparseable frontmatter now fails lint** (iter-174). Files
  that previously vanished silently from the scan (leaving a green
  `0 files checked, no issues`) now surface as `HYALO005` errors and exit 1.
  Vaults that unknowingly contained corrupt files will start failing CI — this
  is intentional: a green lint must mean the vault is genuinely clean.

- **Profile composition now truly composes** (iter-172): merging a profile
  into `.hyalo.toml` unions array keys (`[schema] exempt`, `[lint] ignore`,
  `[schema.default] required`) and dedups `[[schema.bind]]` entries by
  (glob, type) instead of clobbering the previous profile's values; the
  merge is comment- and order-preserving (`toml_edit`) and reports
  `conflict:` lines when a scalar would be overwritten. `[lint] profile`
  (single scalar) is deprecated in favor of the `profiles` list so every
  activated profile's rules fire together; the `--profile` CLI overlay
  composes with file config instead of resetting user additions.
- **Path-bound files satisfy the required-`type` check** (iter-172): a file
  typed via `[[schema.bind]]` (e.g. a frontmatter-less `SKILL.md` or ADR) no
  longer needs an explicit `type:` key to pass `required = ["type"]`,
  including under `--strict`.
- The OKF profile is vendor-neutral (iter-175): the BigQuery example types
  are no longer injected into every vault.
- `hyalo new --type <t>` honors `[schema.types.<t>.defaults]` (e.g.
  `status`, `date = "$today"`) and omits the `type:` key when the target
  path is covered by a `[[schema.bind]]` binding (iter-175).
- `madr toc` excludes files whose explicit `type:` is not `adr` from the
  dashboard instead of listing every `.md` in the directory (iter-175).
- Generated `index.md`/`log.md`/`README.md` managed regions now emit a blank
  line after the begin marker and before the end marker, so a freshly
  generated file passes MD022 — ending the `lint --fix` ↔ `okf index` revert
  ping-pong.
- `okf index` / `okf log` / `madr toc` `--format text` output now renders
  readable per-file lines instead of a mis-nested `files: action: create` key
  dump.
- This repository's own knowledgebase is linted in CI on every PR
  (`lint-kb` job, `hyalo lint --strict --format github`) (iter-170).

### Fixed

- **OKF generator hardening** (iter-176): closes the data-safety and
  output-correctness edges the final pre-release dogfood found in `okf index`/
  `okf log`.
  - *Marker-edge data loss*: an `index.md` with a **dangling / reversed /
    duplicate** `okf:index` managed-region marker is now left byte-identical and
    reported as `skip` (with a stderr warning), never rewritten — the
    generator no longer splices across a broken marker and deletes the hand
    prose after it on a second `--apply`. A new advisory `OKF-INDEX-MARKERS`
    lint rule flags the same condition in CI, and malformed-marker files count
    as drift in `--dry-run`.
  - *CommonMark-valid links*: generated bullets are always valid Markdown link
    items — destinations with spaces are angle-bracket wrapped
    (`](<blocks table.md>)`), `[`/`]` in titles are backslash-escaped, and
    multi-line `description` / titles are collapsed to one line.
  - *Robust apply*: an impossible or unwritable target (e.g. a directory named
    `index.md`) is warned-and-skipped and the run continues writing the other
    files instead of aborting mid-run; `--dry-run` reports `skip` for such
    targets instead of claiming `create`. `okf log` rejects a non-file target
    the same way.
  - *Scope & message polish*: a nonexistent `okf index <dir>` scope is rejected
    (exit 1) instead of vacuously passing; `-q`/`--quiet` now suppresses the
    skip warnings; `okf log` indents multi-line `--message` continuation lines
    so an embedded `## heading` can't corrupt the log; `okf log --action ""`
    errors like `--message ""`; a nonexistent `okf log <dir>` target is
    rejected consistently in dry-run and apply. Grammar: `N file written` and
    `preserving 1 existing line`. Re-running `init --profile <p>` on an
    already-merged config now reports `unchanged` instead of `updated`.
- **Malformed-file policy** (iter-173): `okf index` now skips a concept with
  unparseable frontmatter with a per-file stderr warning and continues, instead
  of aborting the whole run on the first bad file (exit code 2 is reserved for
  real I/O/config errors; drift stays exit 1). A scoped run (`okf index
  <subtree>`) no longer dies on a malformed file elsewhere in the vault.
- `SCHEMA` "missing required property" violations now report
  `autofixable: false` when no schema `default` exists for the property (so
  `--fix` cannot synthesize a value), instead of a misleading `true`.
- **`lint --limit 0` now means unlimited** (iter-174): it previously emptied the
  `files[]` list *and* zeroed the `errors` counter, so `hyalo lint --limit 0` on
  a corrupt vault exited 0 with no findings. `--limit 0` now lifts the file cap
  (matching `--count --limit 0`) and the `errors`/`warnings` counters and exit
  code are computed over the whole vault, never the truncated display slice —
  so a `--limit N` cap can no longer hide an error.
- **`--format github` annotations are no longer truncated by the file cap**: the
  regression is now covered by a test that lints 60 files past the default
  50-file cap and asserts all 60 annotations are emitted.
- **`changelog add` inserts inside `[Unreleased]`** (iter-175, RB-4): the new
  `### Category` is bounded at the footer link-reference block, so entries no
  longer land after the link refs at EOF (which made every conformant Keep a
  Changelog file fail its own lint); output stays MD047-clean.
- `types set default` is rejected with a message pointing at
  `[schema.default]` instead of silently writing a phantom, unused
  `[schema.types.default]` table (iter-175).
- `[schema] exempt` globs and the OKF reserved-file checks (`index.md`/`log.md`)
  now honor the resolved `[links] case_insensitive` mode, so an adopted
  `INDEX.md` on macOS/Windows is exempted and classified as reserved instead of
  failing `lint` as a typeless concept doc.
- Skip-summary pluralization (`1 input path missing`) and YAML parse errors no
  longer leak library-internal advice (`set DuplicateKeyPolicy in Options if
  acceptable`) in `HYALO005` messages and generator skip warnings.
- **`changelog add` no longer splits a wrapped multi-line bullet** (LB-5): when
  the last bullet under a `### Category` had hanging-indent continuation
  lines, the new entry was inserted after only the bullet's first line,
  stranding its continuation lines below the new entry. The insertion anchor
  now scans past a bullet's full continuation block before inserting.

## [0.17.0] - 2026-07-11

### Added

- Linux packages: `.deb` and `.rpm` are built on every release, attached as
  release assets, and published to the hosted apt/yum repos at
  [Cloudsmith](https://cloudsmith.io/~ractive/repos/hyalo)
  (`ractive/hyalo`).
- Shell completions (`hyalo completion <shell>`) are now packaged: included
  in all release archives and installed by the `.deb`/`.rpm` at the
  standard bash/zsh/fish paths.
- CycloneDX SBOMs and GitHub build-provenance attestations for native
  builds.

### Changed

- The release pipeline moved to the shared reusable workflow in
  [ractive/release-workflows](https://github.com/ractive/release-workflows)
  (`@v0.2.0`); `release.yml` is now a thin caller. Release archives are
  named `hyalo-v<version>-<target>.*` (previously unversioned) and include
  `LICENSE` and `README.md`.
- Releases can be rehearsed end to end with a `workflow_dispatch` dry run
  (builds and packages everything, publishes nothing).

### Fixed

- Two `clippy` findings from the Rust 1.97 toolchain (`question_mark`,
  `unneeded_wildcard_pattern`).

## [0.16.1] - 2026-07-10

### Changed

- Release pipeline hardening from the v0.16.0 rollout: `hyalo-mdlint` is now
  published to crates.io (between `hyalo-core` and `hyalo-cli`), duplicate
  publishes are treated as success ("already exists"), a per-target
  `rust-cache` key stops cross containers restoring host-glibc build scripts,
  and a manually-dispatchable `publish-crates.yml` can resume a partial
  crates.io publish without re-running the release matrix.

### Fixed

- Release builds now inject `GIT_COMMIT`/`GIT_COMMIT_DATE` (the hermetic
  provenance path in `build.rs`; `rerun-if-env-changed` forces the build
  script past stale caches), with `Cross.toml` passthrough for containerized
  cross builds. Correction: this was released as a fix for v0.16.0 binaries
  reporting a stale June sha, but the shipped v0.16.0 binaries were verified
  correct after the fact — the report traced to a PATH-shadowed local
  `cargo install` binary. The hardening stands as prevention; the shell-out
  path remains the fallback for local builds.

## [0.16.0] - 2026-07-10

### Added

- **iter-159**: `hyalo init --pi` installs pi skill artifacts
  (`.pi/skills/{hyalo,hyalo-tidy}`, `.pi/extensions/hyalo.ts`,
  `.pi/package.json`); `hyalo deinit` removes them.
- **iter-155**: `datetime` schema property type
  (`YYYY-MM-DDThh:mm:ss`), with `$today` expansion in defaults.
- **iter-156**: `required` properties now reject empty values (`[]`, `~`,
  `""`) — a required `tags` must be non-empty, no separate knob needed.
- **iter-147**: Hardened `--files-from` on `task toggle` / `task set`.
  `--line` is now rejected at clap parse time when combined with
  `--files-from` (line numbers are per-file and don't compose across a
  list), and `--files-from` without `--all` or `--section` returns a
  clear user error. Help-text examples on `task set` now include
  `--files-from` and `--glob` forms (`task toggle` already had them).
- **iter-145**: `task toggle` and `task set` now accept
  `--files-from <file|->` and `--glob <pattern>` via the unified input
  resolver. Multi-file selection flattens all per-file task results into a
  single array in the standard
  `{"results": [...], "total": N, "hints": [...]}` envelope.
- **iter-145**: `task read`, `read`, and `backlinks` now accept
  `--files-from` (resolved to a single file, consistent with their
  single-file policy). `--glob` is explicitly rejected with a clear error
  for these commands.
- **Quality-gate xtask** (`cargo run -p xtask -- check-ac-fidelity |
  check-feature-fanout | check-help-drift`): three PR-time guards that catch
  partial implementations (AC-fidelity), cross-command flag inconsistency
  (feature-fanout matrix), and help-text drift before merge. Wired into a new
  `quality-gates.yml` CI workflow.
- **`EXAMPLES:` blocks on every subcommand `--help`** (`find`, `set`, `task`,
  `summary`, `read`, `links`, `create-index`, `types`, `properties`, `tags`,
  `backlinks`, `remove`, `append`, `views`, `init`, `lint-rules`) —
  LLM-ergonomics fix so agents don't need to escalate to top-level
  `hyalo help` to find idiomatic patterns. An integration test guards against
  future regressions.
- **`--files-from <PATH>`** flag on `find`, `lint`, `mv`, `set`, `remove`, and
  `append`: supply a newline-separated list of file paths (or `-` to read from
  stdin) and the command operates on exactly that set, bypassing the directory
  walk. Non-`.md` paths, paths outside the vault, and missing files are
  silently skipped; counters appear in the JSON envelope as `files_missing`,
  `files_skipped_non_md`, and `files_skipped_outside_vault`. Enables
  diff-aware CI workflows: `git diff --name-only origin/main | hyalo lint
  --files-from -`. Mutually exclusive with `--glob` and `--file`.
- **`item_pattern`** on `string-list` properties: per-item regex validation
  at `hyalo lint` time. Declare `type = "string-list"` and
  `item_pattern = "^..."` in `[schema.types.X.properties.Y]`. Each list item
  is matched against the regex; violations include the item index and pattern.
- **`required-sections`** on type schemas: declares the body outline a
  document of this type must contain. Entries are `"## Heading"` strings
  (level encoded by hash count); order-significant; extras are silently
  allowed. Enforced by `hyalo lint`.
- **`hyalo new --type <name> --file <vault-relative-path>`**: schema-driven
  file scaffolder that emits a placeholder skeleton (required frontmatter +
  required sections, all values `TBD` / type-appropriate empties). Designed to
  produce a file that fails lint — the lint loop is the agent feedback
  mechanism.
- `properties rename --dry-run` and `tags rename --dry-run` — preview which
  files would be modified without writing to disk.
- `find --fields outline` — alias for `--fields sections`.
- `--stemmer` / `--language` now accepts ISO 639-1 two-letter codes (e.g.
  `en`, `de`, `fr`) in addition to full language names.
- `create-index` output now notes when replacing an existing index file.
- `lint` hints now suggest adding unfixable files (e.g. unclosed frontmatter)
  to `[lint] ignore` in `.hyalo.toml` instead of only showing "See defined
  type schemas".
- **Case-insensitive link resolution.** Wikilinks and markdown links now
  resolve even when the target file's path differs in case (e.g.
  `[[api/fetch]]` matches `API/Fetch.md`). Controlled via `.hyalo.toml`:
  `[links] case_insensitive = "auto"` (default), `true`, or `false`.
  `"auto"` enables it on case-insensitive filesystems (macOS, Windows).
- New lint rule `link-case-mismatch`: warns when a link resolves only via
  case-insensitive fallback, suggesting the canonical-case path.
- `links fix` now detects and offers to fix case-mismatched links.
- `task set --dry-run` — preview which tasks would be changed without
  modifying the file.

### Changed

- **Breaking:** the hybrid `--index [=PATH]` flag has been split into two
  orthogonal flags:
  - `--index` is now a pure boolean; no value accepted.
  - `--index-file <PATH>` specifies an explicit index file and implies
    `--index`.

  Migration:

      hyalo find --index=./my.idx
      hyalo find --index-file=./my.idx

  `--index` and `--index-file` are **no longer global** — they appear only on
  subcommands that actually consume the snapshot index (`find`, `summary`,
  `tags summary/rename`, `properties summary/rename`, `backlinks`, `lint`,
  `links fix`, `read`, `set`, `remove`, `append`, `mv`, `task *`). They no
  longer appear on `create-index`, `drop-index`, `init`, `completion`,
  `views *`, or `types *`.
- **Breaking:** `properties rename` and `tags rename` JSON output now uses
  `skipped_count` (integer) instead of `skipped` (array) for consumers that
  parse the JSON output.
- **iter-148** (NEW-5): `hyalo summary --format json` no longer duplicates the
  `dir` field inside `results`. It is now present only at the top-level
  envelope (`.dir`); `.results.dir` is absent. This is a breaking JSON shape
  change — callers must read `.dir` instead of `.results.dir`.
- **iter-157** (performance): the wikilink stem map is lazy and index-seeded —
  indexed queries no longer walk the vault on every invocation (MDN `summary`:
  2.9 s → 0.6 s on a 114 MB index).
- **iter-150**: link-handling refactor unifying wikilink written-form
  preservation across `mv`/`links fix`.
- **iter-148** (NEW-4): `hyalo set --help`, `hyalo remove --help`, and `hyalo
  append --help` now list `--files-from` in the `--file` mutual-exclusion
  sentence. Previously only `--glob` was mentioned; the flag itself already
  worked.
- **iter-146**: `hyalo --version` now includes the git short-sha and commit
  date — e.g. `hyalo 0.16.0 (abc123def456 2026-05-26)`. A `+dirty` suffix is
  appended when the working tree had uncommitted changes at build time.
  Builds without a `.git` directory (crates.io tarball, offline) fall back
  silently to the bare `hyalo <semver>` form. Set
  `CARGO_HYALO_FORCE_NO_GIT=1` to force the bare form; CI can pre-supply
  `GIT_COMMIT` + `GIT_COMMIT_DATE` to skip the shell-out.
- **iter-145**: Unified file-input resolver (`commands/inputs.rs`) replaces
  three separate seams: `resolve_files_from_for_command`, `collect_files`,
  and `resolve_single_file`. All `<FILE>`/`--file` commands now go through
  the single `resolve_inputs` entry point with a per-command
  `ResolutionPolicy` that captures single-vs-multi semantics.
- **iter-144**: Index-suggestion hints. Two new automatic hints surface
  `hyalo create-index` when no snapshot index is active:
  - **Slow-query hint** — fires on `find`, `lint`, `backlinks`,
    `properties summary`, `tags summary`, `summary`, and `read` when the
    command takes longer than 500 ms. Suppressed by `--quiet` or when
    `--index`/`--index-file` is already in use.
  - **Large-vault summary hint** — fires from `hyalo summary` when the
    vault contains more than 500 files and no index is active.

  Both hints count toward the existing `MAX_HINTS` cap and are suppressed
  by `--no-hints` like all other hints.
- **iter-143**: New `hyalo lint` hint — when SCHEMA violations land on a file
  with a declared `type:`, `hyalo types show <T>` is surfaced as the
  next-step. Generic across all SCHEMA failure modes (`required`, `pattern`,
  `item_pattern`, `required_sections`, type-mismatch). Suppressed when
  `--rule SCHEMA` or `--rule-prefix HYALO` is already active. Capped at 2
  distinct types per invocation.
- **iter-143**: `hyalo types show <T>` now suggests `hyalo new --type <T>`
  when the type declares any `required` properties.
- **iter-143**: `--files-from` callers (any command that accepts it) get
  counter-aware advice hints: `<N> input path(s) did not exist on disk` and
  `<N> input path(s) were outside the vault`. Prepended so the `MAX_HINTS`
  cap doesn't crowd them out behind generic next-step hints.
- `--index` semantics: bare `--index` now unambiguously uses `.hyalo-index`
  in the vault directory. Use `--index-file <PATH>` for a non-default path.
- Removed three `unsafe { from_utf8_unchecked }` blocks in the scanner; the
  ASCII-only mutation paths now go through safe `String::from_utf8`. Only
  `unsafe` left in the codebase is `libc::kill(pid, 0)` for PID-liveness in
  the snapshot index. See [decision-log DEC-042] and
  `research/miri-unsafe-audit.md`.
- Internal: Miri scaffolding — `justfile` recipes (`just miri`,
  `just miri-filter`, `just miri-all`) and `#[cfg(not(miri))]` gates around
  `rayon::par_iter` with serial fallback. Manual gate only, not in CI.

### Fixed

- **iter-160 (CRITICAL)**: `lint-rules set --severity/--enabled` no longer
  panics (SIGABRT) when `.hyalo.toml` carries `lint` as a non-table scalar —
  clean JSON error, exit 1, config file untouched.
- **iter-160 (HIGH)**: `links fix --apply` now rewrites `[[wikilinks]]` inside
  frontmatter link properties. Previously frontmatter fixes were reported as
  applied but never written, so fix loops never converged. The JSON envelope
  gains `applied_fixes` / `unapplied` / `unapplied_fixes`, all derived from
  what actually landed on disk; BOM-prefixed files are handled.
- **PR #186**: `hyalo … | head` exits quietly on a broken pipe (SIGPIPE reset
  on Unix + panic-hook backstop, exit 141) instead of panicking.
- **PR #186**: `links auto --first-only` treats an existing `[[wikilink]]` or
  `[markdown](link)` to a target as that target's first mention — plain-text
  mentions after an existing link are no longer double-linked.
- **iter-158** hardening (full-codebase review): BOM/leading-whitespace
  frontmatter corruption on `set`/`remove`/`append`; `lint --fix`
  line/column→byte conversion and non-atomic body writes; `mv` vault escape
  through a symlinked destination; missing file-size caps on `lint`/`read`;
  non-JSON error output under `--format json`; snapshot-index link-graph
  corruption on mutation; BM25 ranking divergence between indexed and scan
  paths; `task toggle --line` mutating checkbox lines inside code fences.
- **iter-152**: frontmatter exceeding the size budget produces a clear
  diagnostic instead of silently dropping the file from all queries.
- **iter-153**: unicode/emoji tags written by `set`/`append` are queryable
  via `find --tag` (write/query symmetry).
- **iter-154 / iter-149**: `mv` and `new` patch an existing snapshot index in
  place instead of leaving it stale.
- **iter-148** (NEW-3): `--files-from` now correctly strips a multi-segment
  `--dir` prefix from repo-relative paths when `--dir` is passed explicitly on
  the CLI. The marquee recipe `git diff --name-only | hyalo --dir files/en-us
  find --files-from -` now resolves entries like `files/en-us/foo.md` to
  `foo.md` inside the vault, with `files_missing=0`. Single-segment and
  dot-dir vaults are not regressed.
- **iter-148** (NEW-1): `hyalo summary` now always includes the `create-index`
  hint on large vaults (>500 files) even when orphan / broken-link / `links
  fix` hints would otherwise fill all `MAX_HINTS=5` slots. The hint is
  prepended (highest priority) rather than appended, so it is visible on real
  large vaults like MDN where health-hint pressure is highest.
- **iter-143**: `--index --files-from` now consults the snapshot for
  membership instead of falling through to `is_file()`. Paths that exist on
  disk but are absent from the snapshot count as `files_missing` —
  consistent with the `--index` contract ("snapshot is the source of
  truth"). Closes the deferred item from iter-139.
- **NEW-1**: `item_pattern` lint validation now reports every offending item
  in a `string-list` property (with its index) instead of short-circuiting
  after the first. Same fix for the per-item "expected string, got <kind>"
  branch.
- **NEW-2**: `--files-from` now strips the full configured `--dir` prefix
  (multi-segment paths like `files/en-us/x.md` with `--dir files/en-us`), not
  just the last component. Forward-slash normalisation handles
  Windows-flavoured input. Vault-relative literal paths still win over
  strip-and-retry. The all-missing stderr hint quotes the actual configured
  `dir`.
- **NEW-3**: `hyalo new --help` no longer claims it errors when the parent
  directory is missing (iter-140 BUG-4 made it `create_dir_all`). Help text
  scrubbed.
- **NEW-4**: `--files-from` trims leading/trailing whitespace per line before
  resolving, so `printf '  edge.md\n'` no longer reports the path as missing.
- **NEW-5**: `create-index` accepts `--index-file PATH` as a synonym for
  `-o/--output`. Conflicting values (`-o A --index-file B`) produce a clear
  error. The stale-index warning no longer fires when output was redirected
  away from the default location.
- **NEW-6**: `--files-from` input is deduplicated by resolved vault-relative
  path, preserving first-seen order (uses `IndexSet`). Pipelines like
  `git log --name-only` no longer cause `lint` to re-lint or `find` to return
  duplicates.
- **BUG-1**: `required_sections` schema enforcement was dead code in the
  grouped lint path (`lint_one_file_extended`). It now calls
  `validate_required_sections` and reports missing or out-of-order sections
  as `SCHEMA` errors.
- **BUG-2**: `--files-from` now strips the vault-dir basename prefix from
  repo-relative paths (e.g. `kb/notes/foo.md` with `--dir kb` resolves to
  `notes/foo.md`). Emits a hint to stderr when every entry was missing.
- **BUG-3**: Canonical TOML key for required body sections is now
  `required_sections` (snake_case). The old `required-sections` (kebab) is
  accepted as a deprecated alias and emits a warning on load.
- **BUG-4**: `hyalo new` now creates parent directories automatically
  (`create_dir_all`) instead of returning an error when they are missing.
- **BUG-5**: `hyalo new` scaffold no longer emits a double trailing newline;
  output ends with exactly one `\n`, eliminating MD047 false positives.
- **BUG-6/7**: `--files-from` counters (`files_missing`,
  `files_skipped_non_md`, `files_skipped_outside_vault`) are now under
  `.results` in the JSON envelope. For `lint` (results is an object) they are
  inserted directly; for `find` (results was a bare array) the array is
  promoted to `{"files": [...], "files_missing": N, ...}`.
- `hyalo backlinks <target.md>` now finds incoming short-form `[[basename]]`
  wikilinks that unambiguously resolve to the target — previously they were
  silently dropped while `find --fields links` resolved them correctly. The
  two commands now share resolver semantics. `find --orphan` / `--dead-end`
  inherit the fix.
- **Cross-platform link resolution.** Obsidian short-form bare wikilink
  resolution (`[[note]]` → `sub/note.md` when unique) now works on
  case-sensitive filesystems (Linux, Windows) even when
  `[links] case_insensitive` is off or auto-detects off. Previously the
  short-form stem fallback was incorrectly gated on case-insensitive mode.
- `links fix` reports a short-form wikilink whose stem casing differs from
  the on-disk filename as `LinkCaseMismatch` (was `ShortFormStemMismatch`).
  Same user intent — fix the casing — and now consistent across platforms.

### Security

- Snapshot index (`.hyalo-index`) now validates entry paths on load —
  rejects traversal (`..`), absolute paths, and null bytes.
- Snapshot index files larger than 512 MB are rejected to prevent OOM from
  crafted files.

[Unreleased]: https://github.com/ractive/hyalo/compare/v0.17.0...HEAD
[0.21.0]: TBD
[0.20.0]: TBD
[0.19.0]: TBD
[0.18.0]: TBD
[0.17.0]: https://github.com/ractive/hyalo/compare/v0.16.1...v0.17.0
[0.16.1]: https://github.com/ractive/hyalo/compare/v0.16.0...v0.16.1
[0.16.0]: https://github.com/ractive/hyalo/compare/v0.15.0...v0.16.0
