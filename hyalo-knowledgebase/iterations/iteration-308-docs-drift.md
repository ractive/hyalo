---
title: "Iteration 308: documentation drift"
type: iteration
date: 2026-10-04
tags:
  - iteration
  - docs
  - review
status: completed
branch: iter-308/docs-drift
---

# Documentation drift

## Problem

[[reviews/codebase-review-2026-10-03]] lists documentation that states
something false, behaviour documented nowhere but `--help`, and two structural
observations: hand-maintained doc sources with no cross-gate, and
`check-jq-recipes` skipping README, `docs/*.md`, the root `CLAUDE.md` and the
KB docs. Every claim below is checked against the release binary before the
text changes; evidence is in the Validation section.

## Tasks

### False claims

- [x] docs/configuration.md: describe the real `.hyalo.toml` ancestor lookup instead of "never walks up"
- [x] docs/configuration.md: read hints from plain `--format json`, not through `--jq` (DEC-313)
- [x] skill-hyalo.md: replace the "npm 0.22.0 is CLI-only" paragraph with the shipped typed API
- [x] skill-hyalo.md: correct the narrowed-rule count and the MD rule range
- [x] `required` semantics and snapshot format version corrected in the claims paragraph, rule, skill and docs
- [x] `links_fuzzy_min_confidence` JSON key named correctly in docs/configuration.md and the rule
- [x] `init --claude` help text names both skills and the rule
- [x] README v0.21.0 sentence, pi-package README "embeds", and moved `iterations/done/` links fixed

### Undocumented behaviour

- [x] CHANGELOG `[Unreleased]` carries iteration 301; docs/ci.md `--strict` matches `lint --help`
- [x] docs/configuration.md documents `[scan]`, `[search]`, `[lint]`, `[views]`, `[changelog]`, `[okf]`, `[links] frontmatter`
- [x] Help-only flags and HYALO003/HYALO004 documented; KB type table lists `datetime-tz`
- [x] Mutating examples carry `--dry-run`; `check-jq-recipes` covers README, docs, root CLAUDE.md and KB docs (DEC-346)

### Structural

- [x] docs/releasing.md "Documentation sources" section; DEC-347 names the claims paragraph canonical

## Acceptance criteria

- [x] Every corrected claim has binary evidence in the Validation section
- [x] `just gates` passes, including the extended `check-jq-recipes`
- [x] `cargo test -p hyalo-cli --test e2e agent_discoverability -q` passes
- [x] `hyalo lint --strict` is clean on every changed KB file, except five pre-existing `decision-log.md` HYALO008 anchors (see follow-ups)

## Behaviour follow-ups

No claim required a behaviour change; every false sentence was corrected to
match the binary. Observations left for a later iteration:

- `hyalo lint --strict --help` (on this branch's base) still describes only the
  missing-type, undeclared-property and HYALO003 promotions; the binary also
  promotes HYALO004, HYALO006, HYALO007 and HYALO008. Iteration 306 rewrites
  that help text, so this iteration only corrected `docs/ci.md` and the docs.
- `decision-log.md` carries five pre-existing `[[decision-log#DEC-NNN]]`
  anchors that HYALO008 reports under `--strict` (headings are
  `DEC-NNN: <title>`); the whole-vault `lint --strict` already fails on `main`
  (19 errors in 13 files). Not touched here.
- `.gitignore` is honoured only inside a git repository, while `.ignore` is
  honoured everywhere (the `ignore` crate's `require_git` default). Now
  documented; whether hyalo should say so in `--help` or count the hidden
  files belongs to the review's F5 follow-up.
- `hyalo init` has no `--dry-run`, so "preview what `--claude` installs"
  needs a scratch directory.

## Validation

All evidence from `target/release/hyalo` built from this branch
(0.24.1, base `3bdf2630`) in scratch vaults under the session scratchpad.

1. Ancestor config: from `kb/sub` with `dir = "kb"` in the parent,
   `hyalo config` printed `note: using …/.hyalo.toml from a parent directory:
   the vault is …/kb, not the current directory — pass --dir . to scope this
   run to the current directory` and `config: …/.hyalo.toml`
   (`discover_ancestor_config`, nearest config wins, adopted only when its vault
   contains the cwd).
2. Hints: `hyalo find --format json | jq '.hints|length'` → `3`;
   `hyalo find --format json --jq '.hints'` → `[]`.
3. npm API: `npm/hyalo/package.json` exports `./dist/index.{mjs,cjs}`;
   CHANGELOG 0.23.0 "now includes a typed TypeScript API for `find`, `read`,
   `summary`, and `config`"; `terms`/`tags`/`backlinks` are in `[Unreleased]`
   (iteration 307, untagged).
4. `hyalo lint-rules list` ends at `MD060` with gaps (no MD008, MD015–MD017,
   MD057). `DESCRIPTION_SUFFIX` in `hyalo-mdlint/src/engine.rs` carries hyalo
   deviations for five stock rules: MD001, MD018, MD034, MD047, MD042 (the KB
   table in `docs/schema-and-lint.md` already listed five).
5. `required = ["title", "owner"]`: `title: ""` and `title: "  "` pass;
   `title:` (null) and `title: []` → `required property "title" must not be
   empty`; absent → `missing required property "title"`.
   `hyalo config --jq .results.snapshot_format_version` → `4`.
6. `hyalo config --format json --jq '.results | keys'` contains
   `links_fuzzy_min_confidence` (no `links.fuzzy_min_confidence`); text output
   prints `links.fuzzy_min_confidence: 0.8`.
7. `hyalo init --claude` in an empty directory created `.claude/skills/hyalo/SKILL.md`,
   `.claude/skills/hyalo-tidy/SKILL.md` (+ Jev assets), `.claude/rules/knowledgebase.md`
   and `.claude/CLAUDE.md`. `init --pi` likewise writes both skills, the extension
   and `lib/hyalo-api.{js,d.ts}`, so its help line was corrected too.
   `cargo test -p hyalo-cli --test e2e agent_discoverability -q` → 21 passed.
8. README's pi section pinned `@v0.21.0` while telling readers to pin their
   binary's tag; `crates/hyalo-cli/src/commands/init.rs` embeds
   `templates/pi/…`, the vendored copy `check-pi-package-sync` compares.
   `iterations/iteration-16-robustness.md` and `iterations/iteration-101-bm25.md`
   exist only as `iterations/done/iteration-16-robustness.md` and
   `iterations/done/iteration-101-bm25-ranked-search.md`.
9. Scratch vault with a `date: April 9`, an undeclared key, `[[b]]` and
   `[[a#nope]]`: default lint warns SCHEMA/HYALO003/HYALO006/HYALO008/MD009;
   `--strict` turns all but MD009 into errors. Hidden-target fixture:
   `[ig](.gitignore)` and `[ci](.github/workflows/ci.yml)` resolve, only
   `.github/missing.yml` is HYALO006; `links fix --dry-run` reports
   `anchor_fixes: [{old_fragment: "success-metrics", new_fragment:
   "6-success-metrics", heading: "6. Success metrics"}]`.
10. Every ```` ```toml ```` block in `docs/configuration.md` (11 blocks) was
    written to a scratch `.hyalo.toml`; `hyalo config --format json` reported
    no `malformed`/`schema_error` for any, and `hyalo lint` on the top block
    and the `[lint]` block exits 0. `views set drafts --property status=draft
    --tag rust` writes `[views.drafts] properties = ["status=draft"]
    tag = ["rust"]`; `lint-rules set MD013 --severity bogus` → exit 1 "severity
    must be 'warn' or 'error'". `[scan]` fixture: outside git `g.md`
    (gitignored) is found and `i.md` (`.ignore`) is not; after `git init` both
    are hidden; `include = [".hidden/**"]` admits `.hidden/h.md`.
11. Each flag exists in `--help`: `summary -n/--recent`, `summary --depth`,
    `find --filenames0`, `find --reverse [alias: --desc]`,
    `lint-rules list --enabled-only/--disabled-only`,
    `links fix --expand-short-form`, `task set -s/--status`,
    `changelog add --wrap <COLS>`. HYALO003 fires without a schema on `date`
    (`DATE_KEYS = date, created, modified, updated`); HYALO004 fires on a naive
    value in a `datetime-tz` property and accepts `…Z`.
12. Extended `check-jq-recipes`: 47 recipes execute; unit tests
    `flags_a_filter_that_reads_hints` and
    `covers_readme_root_claude_md_and_both_docs_trees` pass. A first run flagged
    the new `docs/releasing.md` prose `hyalo … --jq` (exit 2), reworded.
13. `docs/releasing.md` "Documentation sources" lists each surface with the
    gate named in `justfile`'s `gates` recipe.

Gates: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace -q` (all suites pass, e2e 2385), `just gates`.
