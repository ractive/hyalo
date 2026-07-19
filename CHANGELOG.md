# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Maintained with
`hyalo lint --profile changelog` and `hyalo changelog release`/`add`.

## [Unreleased]

### Added

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

### Changed

- **`hyalo links fix` dry-run validates plans against on-disk text** (iter-187):
  dry-run now runs the identical plan-building phase as `--apply`, so its
  `unapplied` / `unapplied_fixes` fields report exactly the fixes `--apply`
  would refuse (stale index / concurrent edit) instead of always being empty.
  The "Apply N fixes" hint count now discounts would-be-stale fixes so it
  matches what `--apply` actually writes.

### Internal

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
[0.19.0]: TBD
[0.18.0]: TBD
[0.17.0]: https://github.com/ractive/hyalo/compare/v0.16.1...v0.17.0
[0.16.1]: https://github.com/ractive/hyalo/compare/v0.16.0...v0.16.1
[0.16.0]: https://github.com/ractive/hyalo/compare/v0.15.0...v0.16.0
