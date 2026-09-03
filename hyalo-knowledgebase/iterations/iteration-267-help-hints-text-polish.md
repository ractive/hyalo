---
type: iteration
title: Iteration 267 — Help, hints and text-output polish sweep
date: 2026-09-03
status: planned
tags:
  - iteration
  - help
  - hints
  - ux
  - dogfooding
branch: iter-267/help-hints-text-polish
priority: 7
related:
  - "[[dogfood-results/dogfood-v0220-obsidian-vaults]]"
  - "[[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]]"
---

# Iteration 267 — Help, hints and text-output polish sweep

## Goal

Close the help, hint and text-output findings left over from
[[dogfood-results/dogfood-v0220-obsidian-vaults]] and the still-open items of
[[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]]. The COMMON
MISTAKES block in `find --help` is wrong on two points (BUG-25). A second
unquoted positional becomes a FILE target and fails with `file not found`
instead of suggesting quotes (UX-3). Files without a `title` property or H1
print `title: (none)` and sort uselessly, where Obsidian shows the filename
(UX-5). `links auto` dry-run on a plugin vault is drowned by common-word
titles such as `github` and `links` (UX-9). Empty-state output and
stdout/stderr interleave are inconsistent (UX-13, COH-17), bulk-mutation text
lists files unindented (UX-14), `hyalo new` is the only writer without
`--dry-run` and scaffolds a plausible `0` for required numbers (UX-17), and a
list of help and hint wording defects (UX-18). From the previous report:
`summary -h` names no result keys (HELP-14) and a file named explicitly on
`lint` that falls under `[lint] ignore` has no override (UX-4 of that report).

Constraint: **no new CLI flags** from dogfood pressure (project rule).
`hyalo new --dry-run` is parity with an existing flag on every other writer
and gets a DEC to justify it; the `links auto` stop-list is a built-in default
plus the existing `[links.auto] exclude_titles`; the lint-ignore override is a
policy on named files, not a `--force`. Out of scope: `find` filter/sort
semantics ([[iterations/iteration-264-find-sort-filter-consistency]]), the
skipped-file summary line and `[scan] exclude`
([[iterations/iteration-265-scan-exclude-and-skipped-files]]), and the text
rendering of list-of-wikilink values (UX-11, in
[[iterations/iteration-262-frontmatter-wikilinks-first-class]]).

## Tasks

### HELP-1: stale help text and result keys (BUG-25, HELP-14, UX-18)

- [ ] `find --help` COMMON MISTAKES: rewrite the `=~` entry to match the
      parser after iteration 264 (it is now an error) and the `title~=`
      entry to say it matches the promoted title (frontmatter or H1). Remove
      the leading NBSP characters in the dot-path paragraph (5 occurrences).
- [ ] `summary -h` and `--help`: list the JSON result keys
      (`results.files.total`, `.links.broken`, `.links.broken_anchors`,
      `.orphans`, `.properties`, `.tags`, plus the iteration-265 `skipped` /
      `excluded`) with one example `--jq`.
- [ ] UX-18 wording: root `-h` preamble when `dir = "."` must not say "don't
      cd into it"; MDN-style `find` without `--index` when `.hyalo-index`
      exists hints `--index`, not `create-index`; `types remove note` error
      when `note` is a built-in default type explains it cannot be removed
      only overridden; `lint --format github` summary counts files checked
      like text does; `hyalo config --format json` carries effective defaults
      for `hints` and `format`; clap errors print `error:` lowercase like
      anyhow ones (set clap's error prefix or post-process).
- [ ] Run the xtask help-drift gate after every help change and keep the
      52-page `Global:` pointer line identical.
- [ ] e2e assertions for each wording fix (grep on `-h` output, `config
      --jq '.results.hints'` non-null); changelog entry.

### HINT-1: second positional and empty-state consistency (UX-3, UX-13, COH-17)

- [ ] hyalo-cli `find`: when a second positional does not exist on disk,
      contains no path separator and does not end in `.md`, fail with `error:
      'plugin' is not a file; did you mean hyalo find 'dataview plugin'?`
      instead of the generic `file not found`. Keep existing behaviour when
      the second positional looks like a path. Consider the reverse hint: a
      single positional ending in `.md` that exists as a file adds a hint
      `-> hyalo find --file <path>` after the body-search results.
- [ ] Zero-result ordering (COH-17): print `No results for …` on stdout before
      the hints, or move both to the same stream, so a terminal shows the
      reason above the suggestion; filter-only zero results and `find '[['`
      behave the same.
- [ ] Empty states (UX-13): `hyalo index` did-you-mean suggests
      `create-index`; `types list` with no types prints `No types configured`
      and then the hint, no blank line; `set --index` no-op does not print a
      stale-index warning for the staleness it repairs itself (coordinate
      with DEC-280 in iteration 265).
- [ ] e2e in `crates/hyalo-cli/tests/e2e` for the second-positional message,
      the zero-result stream order (capture stdout and stderr separately and
      assert order via a combined pty-less check on stdout alone), and the
      three empty states.
- [ ] Docs: `find -h` usage line changes from `[FILE]...` if the DEC below
      says so; skill file COMMON MISTAKES mirror; changelog.

### TITLE-1: filename stem fallback (UX-5)

- [ ] DEC-283 (tentative): when a file has neither a scalar `title` property
      nor an H1, `title` is the filename stem (Obsidian behaviour). JSON adds
      `title_source: "property" | "h1" | "filename"` so consumers can tell
      them apart; `--sort title` and `--property 'title~=…'` use the promoted
      value. HYALO007 is unaffected. Record the impact on the own-KB `find
      --sort title` ordering and on the iteration-258 body-probe hint.
- [ ] hyalo-core title promotion; unit tests for the three sources and for
      Unicode/emoji stems (`🗂️ hub.md`); e2e snapshot on `find --format text`
      showing no `title: (none)`.
- [ ] Docs: `find --help` title paragraph, `.claude/skills/hyalo/SKILL.md`
      title bullet, `.claude/CLAUDE.md`, changelog.

### TEXT-1: bulk-mutation indentation and `links auto` stop-list (UX-14, UX-9)

- [ ] hyalo-cli text renderer for `set`, `remove`, `append`, `properties
      rename`, `tags rename`, batch `mv`: every file under `modified:` /
      `skipped:` is indented two spaces on its own line, so continuation
      lines cannot be read as new keys. Unit test on the renderer.
- [ ] `links auto`: a built-in default stop-list of common-word titles
      (proposal: titles that are single dictionary words of ≤ 6 letters or
      appear in the existing `warn_common_titles` heuristic) is applied
      unless `[links.auto] exclude_titles` overrides it; the run reports
      `default_excluded_titles` so the list is visible. No new flag: the
      existing `--exclude-title` and config lists compose with it.
- [ ] e2e: `links auto --dry-run` on a fixture with notes titled `github`,
      `links`, `Markdown` and `Dataview` matches only `Dataview`.
- [ ] Docs: `links auto --help` stop-list paragraph, skill file, changelog.

### NEW-1: `hyalo new --dry-run` and honest placeholders (UX-17)

- [ ] DEC-285 (tentative): `new` gains `--dry-run` because DEC-257 makes
      `dry_run` universal on object-shaped mutation results and every other
      writer already has the flag; this is parity, not new surface. In the
      same DEC: required `number`/`date`/`bool` properties scaffold as `null`
      (or empty) rather than `0`/today/`false`, so `hyalo lint` flags them
      as unfilled instead of accepting plausible fake values; `string` keeps
      `TBD`.
- [ ] hyalo-cli `new`: implement `--dry-run` (prints the scaffold, writes
      nothing, `dry_run: true` in JSON) and the placeholder change.
- [ ] e2e: `new --type iteration --file x.md --dry-run` writes nothing and
      returns the scaffold; a schema with `rating: number` scaffolds `rating:`
      empty and `lint --file x.md` reports it.
- [ ] Docs: `new -h/--help`, skill file `new` bullet, `.claude/CLAUDE.md`,
      changelog.

### LINT-IGNORE-1: named files vs `[lint] ignore` (UX-4 of the previous report)

- [ ] DEC-284 (tentative): a file named explicitly with `--file` or via
      `--files-from` bypasses `[lint] ignore` and is linted, because naming a
      file is a stronger signal than a glob; `--glob` and the bare vault scan
      keep honouring the ignore list. The existing warning `N named file(s)
      excluded by [lint] ignore` disappears in favour of linting them. Note the
      CI implication: `git diff --name-only | hyalo lint --files-from -` will
      now lint ignored files that changed, which is the desired behaviour for
      a diff gate; document the opt-out (`--glob` instead).
- [ ] Implement in hyalo-cli `lint` target resolution; e2e for `--file`,
      `--files-from`, `--glob` against an ignored path.
- [ ] Docs: `lint --help` ignore paragraph, the `.hyalo.toml` reference page,
      skill file, `.claude/CLAUDE.md` diff-aware lint bullet, changelog.

## Acceptance criteria

- [ ] Obsidian Hub, cwd `../obsidian-hub`: `hyalo find dataview plugin; echo
      $?` → exit 2 and the message names `hyalo find 'dataview plugin'`;
      `hyalo find --limit 3 --format text --no-hints | grep -c 'title: (none)'`
      → 0 and `hyalo find --file '🗂️ hub.md' --format json --jq
      '.results[0] | {title,title_source}'` → `🗂️ hub`, `h1` (or `filename`
      for a file with no H1).
- [ ] `../obsidian-hub`: `hyalo links auto --dry-run --format json --jq
      '.results.matched'` is well below 18510 and `--jq
      '.results.default_excluded_titles'` contains `github` and `links`.
- [ ] Own KB: `hyalo find --help | grep -c $'\xc2\xa0'` → 0; `hyalo find
      --help` COMMON MISTAKES no longer calls `=~` silently accepted nor
      says `title~=` searches only frontmatter; `hyalo summary -h | grep -c
      'results.files'` ≥ 1; `hyalo summary --jq '.results.files.total'`
      matches the text `Files:` line.
- [ ] Own KB: `hyalo find --property status=nonexistent --format text`
      prints the `No results` line before any hint when stdout and stderr are
      merged (`2>&1`); `hyalo index 2>&1 | grep create-index` → 1 line.
- [ ] Own KB: `hyalo lint --file dogfood-results/dogfood-v0220-obsidian-vaults.md
      --count` lints the file (it is under `[lint] ignore`) and reports its
      findings; `hyalo lint --glob 'dogfood-results/*.md' --count` still reports
      `ignored by [lint] ignore`.
- [ ] `hyalo new --type iteration --file iterations/iteration-999-x.md
      --dry-run --format json --jq '.results.dry_run'` → `true` and the file
      does not exist afterwards; `hyalo set … --format text` on three files
      indents each under `modified:`.
- [ ] `hyalo config --format json --jq '.results | {hints,format}'` → both
      non-null; `hyalo lint --format github` summary line counts files checked.
- [ ] `cargo xtask check-help` (the help-drift gate) passes; `hyalo -h` shows
      52 identical `Global:` pointer lines across subcommands.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`, `hyalo lint --strict` on the KB.
- [ ] Changelog entries via `hyalo changelog add`; DEC-283, DEC-284 and
      DEC-285 recorded in [[decision-log]]; `.claude/skills/hyalo/SKILL.md`
      and `.claude/CLAUDE.md` updated for the title fallback, `new --dry-run`
      and the named-file lint policy.

## Links

- [[dogfood-results/dogfood-v0220-obsidian-vaults]]
- [[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]]
- [[iterations/iteration-264-find-sort-filter-consistency]]
- [[iterations/iteration-265-scan-exclude-and-skipped-files]]
- [[decision-log]]
