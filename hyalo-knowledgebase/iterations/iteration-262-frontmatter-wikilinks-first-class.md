---
type: iteration
title: Iteration 262 — Frontmatter wikilinks as first-class links
date: 2026-09-03
status: planned
tags:
  - iteration
  - links
  - frontmatter
  - obsidian
  - dogfooding
branch: iter-262/frontmatter-wikilinks-first-class
priority: 2
related:
  - "[[dogfood-results/dogfood-v0220-obsidian-vaults]]"
---

# Iteration 262 — Frontmatter wikilinks as first-class links

## Goal

Obsidian treats every `[[wikilink]]` inside a frontmatter value as a graph
edge; hyalo today counts only the `related:` key (BUG-1 in
[[dogfood-results/dogfood-v0220-obsidian-vaults]]). On kepano-obsidian that
makes `backlinks Categories/Books.md` empty although three files carry
`categories: ["[[Books]]"]`, lists 25 orphans that are all linked through
`categories:`/`type:`/`status:` values, and lets `mv Categories/Books.md`
report `total_links_updated: 0` while silently breaking the links hyalo itself
counted a moment earlier. This iteration makes every frontmatter string and
list-of-string value containing `[[…]]` a link for backlinks, orphans,
dead-ends, `summary`, `--broken-links` and HYALO006, with a `[links]
frontmatter = false` opt-out in `.hyalo.toml`, and teaches `mv` to rewrite
them in place. It also fixes the three text-output problems found on the same
vault: `mv` hides its counters (UX-4), list-of-wikilink values render as
`[[[People]]]` (UX-11), and `set` silently collapses a list property to a
scalar (UX-12).

Constraint: **no new CLI flags** from dogfood pressure (project rule). The
opt-out is a config key; the `set` list-collapse fix is a warning or a
preserved list, not a `--keep-list` flag. Out of scope: link-target resolution
rules (schemes, attachments, case) which land first in
[[iterations/iteration-261-link-resolution-obsidian-compat]], schema binding on
`type: ["[[Authors]]"]` (BUG-13, in
[[iterations/iteration-266-properties-tags-schema-mutations]]), and `links auto`
inserting links into frontmatter (never).

## Carry-over from iteration 261

[[iterations/iteration-261-link-resolution-obsidian-compat]] merged (PR #304)
with everything in its own scope landed — nothing unticked, nothing deferred.
Its one item explicitly routed here was link-target resolution rules
(schemes, attachments, case), which iter-261 kept and this plan already
excludes under "Out of scope" above; FM-1 already assumes iter-261's `kind`
field is in place (`kind: "frontmatter"` complements it) and 261's e2e fixture
`[[AidenLx]]` → `People/aidenlx.md` case-folding is the resolver behaviour
this iteration's frontmatter links inherit unchanged. No scope change needed
here as a result of iter-261's actual outcome.

## Tasks

### FM-1: scan every frontmatter value for wikilinks (BUG-1)

- [ ] hyalo-core link extraction: walk the parsed frontmatter and extract
      `[[target]]`, `[[target|alias]]`, `[[target#anchor]]` from every string
      scalar and every string item of a list, at any nesting depth, not only
      `related`. Each link records `line` (the frontmatter line, 1-based, as
      the body links do), `kind: "frontmatter"` (additive, complements the
      `kind` field from iteration 261) and the property key it came from.
- [ ] Wire the new edges into the graph used by `backlinks`, `find --orphan`,
      `find --dead-end`, `find --broken-links`, `summary.links`, HYALO006, and
      the `--sort links_count|backlinks_count` keys. Keep body links and
      frontmatter links in one list under `--fields links`.
- [ ] `[links] frontmatter = false` in `.hyalo.toml` restores today's
      behaviour; `related:` still counts when the switch is off, so no vault
      regresses. `hyalo config` prints the effective value. DEC-269
      (tentative): frontmatter wikilinks are graph edges by default, with the
      config opt-out, and `related` is no longer special-cased.
- [ ] Snapshot index: frontmatter links must be stored and served by `--index`
      identically to the disk path (parity test on scores and on
      `backlinks --index`).
- [ ] Unit tests in hyalo-core: scalar, flow list, block list, nested map,
      quoted vs unquoted, alias, anchor, `[[x]]` inside a longer string, and a
      value that is not a link (`"[[not closed"`). e2e in
      `crates/hyalo-cli/tests/e2e`: a three-file vault where `backlinks`,
      `--orphan` and `summary` change exactly as expected, and the same run
      with `frontmatter = false`.
- [ ] Perf: `summary` on Obsidian Hub (6520 files) must stay within noise of
      the report's 0.42 s; frontmatter is already parsed, so only the walk is
      new.

### FM-2: `mv` rewrites frontmatter wikilinks (BUG-1, UX-4)

- [ ] hyalo-core `mv`: for each affected frontmatter link, replace the target
      text inside the existing YAML scalar, preserving the quoting style
      (`"[[Books]]"`, `'[[Books]]'`, bare `[[Books]]` in a block list) and the
      surrounding bytes. This is a text replacement on the frontmatter block,
      not a re-serialisation, so `git diff` shows only the changed target.
- [ ] When a value cannot be rewritten safely (folded/literal block scalars,
      multi-line strings, a target spanning a line break), leave it alone and
      print `warning: N frontmatter wikilinks not rewritten (see --format json
      for the files)`; JSON lists them under `frontmatter_links_skipped`.
- [ ] Text output for single and batch `mv` prints
      `files updated: N, links updated: M` under the `Moved …` line, in both
      real and `[dry-run]` mode, so a silent 0 is visible.
- [ ] e2e: `mv Categories/Books.md Categories/Library.md` on a fixture with
      `categories: ["[[Books]]"]`, a block-list `related:`, and a scalar
      `type: "[[Books]]"` rewrites all three; a folded-scalar fixture triggers
      the warning; the text output carries both counters.
- [ ] Docs: `mv -h/--help` output description, changelog, skill file paragraph
      on `mv`.

### FM-3: list-of-wikilink rendering in text output (UX-11)

- [ ] hyalo-cli text renderer: a list value renders as `["[[Futurism]]",
      "[[Nonfiction]]"]` (JSON-like, quoted items) or one `- item` per line
      when the value contains a `[[`; choose one, apply it to `find`, `read
      --frontmatter`, `properties` and `set` echo consistently. Today `genre:
      [[[Futurism]], [[Nonfiction]]]` is unreadable.
- [ ] Unit test on the renderer with a list of wikilinks, a list of plain
      strings, and a nested list; e2e snapshot on `find --file … --format
      text`.

### FM-4: `set` on a list property (UX-12)

- [ ] DEC-270 (tentative): when `set K=V` targets an existing list property
      with a scalar value, either (a) write the scalar and print
      `note: K was a list in N files; use hyalo append to keep it a list`, or
      (b) preserve the list shape and write a one-element list. Recommend (a)
      because `set` means replace and Obsidian shows the type conflict either
      way; record the choice and the `--validate` interaction (a schema that
      declares `K: list` must reject the scalar).
- [ ] Implement the choice in hyalo-cli `set`, with the note on stderr in text
      mode and `list_collapsed: [files]` in JSON.
- [ ] e2e: `set 'Clippings/Buy wisely.md' --property 'status=[[Draft]]'`
      fixture from the report; `--dry-run` shows the same note.
- [ ] Docs: `set --help` semantics paragraph, skill file, changelog.

## Acceptance criteria

- [ ] kepano-obsidian, cwd `../kepano-obsidian`, clean checkout:
      `hyalo backlinks Categories/Books.md --format json --jq '.total'` → `3`
      (the files whose `categories:` holds `[[Books]]`), and each entry carries
      `kind: "frontmatter"`.
- [ ] `../kepano-obsidian`: `hyalo summary --format json --jq
      '.results.orphans'` drops from 25 to the number of files with no inbound
      body or frontmatter link (expected single digits; name the residue).
- [ ] `../kepano-obsidian`: `hyalo mv Categories/Books.md Categories/Library.md
      --format json --jq '{total_files_updated,total_links_updated}'` → both
      non-zero and `git diff --stat` touches exactly those files with
      one-line hunks changing `[[Books]]` to `[[Library]]` inside quotes;
      `git checkout .` afterwards. Text mode of the same command prints the
      two counters.
- [ ] `../kepano-obsidian`: `hyalo find --file 'References/Bass on Top.md'
      --format text --no-hints` renders every list-of-wikilink property without
      the `[[[` artefact.
- [ ] `../kepano-obsidian`: with `[links]\nfrontmatter = false` appended to a
      scratch `.hyalo.toml`, `hyalo backlinks Categories/Books.md --count` → 0
      and `hyalo config --jq '.results.links.frontmatter'` → `false`.
- [ ] Own KB: `hyalo summary` orphan and broken counts are recomputed; any
      change is explained in the changelog entry (iteration files link each
      other through `related:` and `depends-on:` today, so backlink counts
      will rise).
- [ ] e2e suite covers FM-1 through FM-4; `cargo test --workspace -q` green.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`, `hyalo lint --strict` on the KB,
      xtask help-drift check.
- [ ] Changelog entry via `hyalo changelog add`; DEC-269 and DEC-270 recorded
      in [[decision-log]]; `.claude/skills/hyalo/SKILL.md` and
      `.claude/CLAUDE.md` mention frontmatter links and the config key.

## Links

- [[dogfood-results/dogfood-v0220-obsidian-vaults]]
- [[iterations/iteration-261-link-resolution-obsidian-compat]]
- [[iterations/iteration-266-properties-tags-schema-mutations]]
- [[decision-log]]
