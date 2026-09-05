---
type: iteration
title: "Iteration 272 — Autofix and link-rewrite corruption: MD031, MD019 in code blocks, site_prefix case rewrites, frontmatter ambiguity"
date: 2026-09-05
status: planned
tags:
  - iteration
  - lint
  - links
  - mv
  - dogfooding
branch: iter-272/autofix-and-rewrite-corruption
priority: 2
related:
  - "[[dogfood-results/dogfood-v0220-post-batch-261-270]]"
  - "[[iterations/iteration-263-lint-autofix-obsidian-safety]]"
  - "[[iterations/iteration-269-mv-frontmatter-link-scan-gap]]"
  - "[[decision-log]]"
---

# Iteration 272 — Autofix and link-rewrite corruption

## Goal

Four ways a hyalo *mutation* rewrites content it should have left alone, all from
[[dogfood-results/dogfood-v0220-post-batch-261-270]], all exit 0. Three are `lint --fix` or
`links fix` corrupting a corpus; the fourth is `mv` half-retargeting a file. Group 2 of the
report's recommended iterations. Parts A and B live in `hyalo-mdlint`, C and D in
`hyalo-core`; they share nothing but the theme, and each has a fixture from the report.

Constraint: **no new CLI flags**. Every fix narrows what an existing autofix touches or makes
an existing dry-run honest.

## Part A — MD031 must not fire at the opener of an unterminated fence (BUG-3, HIGH)

```text
printf -- '---\ntitle: t\n---\n\n# T\n\nIntro.\n\n```yaml\n  - uses: x\n  - name: y\n' > unterm.md
hyalo lint --fix --file unterm.md      # fixed MD031 line 9
sed -n '9,11p' unterm.md               # ```yaml / <blank> / "  - uses: x"
```

A fence that never closes runs to EOF; hyalo treats the opener as a closer and inserts a blank
line *inside* the sample. Real hit: `../docs/content/actions/tutorials/build-and-test-code/rust.md`.
Six GitHub Docs files have an odd fence count; markdownlint reports nothing at such an opener.

### MD031-1

- [ ] Root-cause: is the misclassification upstream in `mdbook-lint-rulesets` MD031 or in
      hyalo's fence tracking? If upstream, post-filter in `crates/hyalo-mdlint/src/rules/`
      like the iteration-263 MD034/MD042 filters: drop any MD031 "followed by" finding whose
      fence has no closer before EOF.
- [ ] Unit test with the fixture above plus a terminated fence that must still be fixed.
- [ ] e2e: `lint --fix` on the fixture leaves it byte-identical; on
      `../docs/content` (read-only, `--dry-run`) the six odd-fence files get no MD031 proposal
      at their opener line — list the six paths and their opener lines in the test's comment.

## Part B — MD019 fires and autofixes inside code blocks; audit every autofixable rule (BUG-28, HIGH)

```text
printf -- '---\ntitle: d\n---\n\n# T\n\n```text\n#   three\n```\n\n~~~sh\n#   tilde\n~~~\n\n    #   indented\n' > d.md
hyalo lint --fix --file d.md           # fixed 3 — every "#   x" inside the code becomes "# x"
```

MD018, MD023 and MD026 respect code blocks in the same file; MD019 does not, on backtick
fences, tilde fences and indented blocks alike. Pre-existing (the Sept 1 binary matches), so
iteration 263's fence awareness never reached this rule. It rewrote a sample inside the dogfood
report itself.

### MD019-1

- [ ] Make MD019 skip lines inside fenced (``` and ~~~, any info string) and indented code
      blocks. Prefer one shared "inside a code block" span computation in the engine that any
      rule can consult over a rule-local reimplementation — check whether iteration 263's
      `rules/obsidian.rs` already has the span scanner and lift it.
- [ ] **Audit**: build one fixture that places every autofixable rule's trigger inside a
      backtick fence, a tilde fence, an indented block, an HTML comment, and the frontmatter
      block (`hyalo lint-rules list --jq '.results[] | select(.autofixable) | .id'` gives the
      rule list; write the trigger for each). Assert `lint --fix` leaves the fixture
      byte-identical. Record in the Outcome section which rules, if any, needed the same fix as
      MD019 — rules that are *about* fences (MD031, MD040, MD046, MD048) are exempt from the
      fence part by nature and must be handled explicitly in the fixture.
- [ ] Ship the fixture as an e2e test so the property holds going forward.
- [ ] Re-measure `lint --fix --dry-run` on `../obsidian-hub` before/after (24044 proposals at
      the last dogfood) and report the delta by rule.

## Part C — `links fix` case-mismatch rewrites on a `site_prefix` vault (BUG-4, HIGH)

On a copy of `../mdn/files/en-us/web/css` with `site_prefix = "en-US/docs/Web/CSS"`:

```text
hyalo links fix --dry-run --jq '.results.case_mismatch_fixes[0]'
# old_target "/en-US/docs/Web/CSS/Guides/Anchor_positioning", new_target "guides/anchor_positioning/index.md"
hyalo links fix --apply    # 5096 links in 1049 files become /en-US/docs/Web/CSS/guides/anchor_positioning/index
```

Three defects: the written URL carries a trailing `/index` nobody publishes; the dry-run shows
a vault-relative path while apply writes site-absolute, so the preview does not describe the
rewrite; and DEC-267 calls `link-case-mismatch` cosmetic while it rewrote 5096 links in a corpus
whose URL convention is Title-case over lowercase folders.

### CASE-1: the rewrite keeps the incoming form

- [ ] A case-mismatch plan for a link that resolved through `site_prefix` and/or a directory
      index (`<dir>/index.md`) must write the link back in the form it came in — site-absolute
      stays site-absolute, a directory link stays a directory link — with only the case of the
      path segments changed. Never append `/index` or `.md` to a form that did not have it.
- [ ] Decide, and record in [[decision-log]] as an amendment to DEC-267: should case-mismatch
      plans be produced at all when the on-disk case differs from the link only because the
      site's URL convention differs from its folder convention (MDN: `Web/CSS/Guides` vs
      `web/css/guides`)? Options: (a) keep producing them but with the correct rewrite;
      (b) skip case-mismatch plans for links that resolved via `site_prefix`, since the link is
      correct for the site. (b) is the conservative choice and matches "cosmetic"; pick it
      unless a testbed shows a real need for (a).

### CASE-2: the dry-run shows the exact string that will be written

- [ ] `new_target` (and text-mode output) in `--dry-run` must equal the replacement text
      `--apply` writes, for every strategy, not just case mismatch. Add a test that runs
      `--dry-run` then `--apply` on the same fixture and asserts each applied replacement's
      `new_text` equals the dry-run's `new_target` for the same `(file, line, old_target)`.
- [ ] Measure on the css copy: after the fix `git diff` after `--apply` contains only
      case-only changes (or no changes under option (b)); `case_mismatches` count reported.

## Part D — `mv` must apply the ambiguity guard to frontmatter links (BUG-7, MEDIUM)

```text
mkdir x; printf -- '---\ntitle: A\n---\n' > a.md; printf -- '---\ntitle: XA\n---\n' > x/a.md
printf -- '---\ntitle: C\nrelated: "[[a]]"\nrel2: [[a|al]]\n---\nbody [[a]] and [[a|al]]\n' > c.md
hyalo mv a.md z.md    # body links skipped as ambiguous; related/rel2 rewritten to [[z]]
```

DEC-269 made frontmatter links graph edges; the DEC-288 ambiguity guard only covers body links,
so one file ends half-retargeted with no warning.

### AMBIG-1

- [ ] Apply the same ambiguity test to frontmatter link sources in `plan_mv`; skip them with
      the same `note: skipped ambiguous link … at c.md:<line>` and count them in
      `skipped_ambiguous` alongside body links (add the frontmatter `property` to the record).
- [ ] `--allow-ambiguous` rewrites them too, and says so.
- [ ] e2e with the fixture: `files updated: 0`, four ambiguous notes, bytes unchanged; with
      `--allow-ambiguous`: all four rewritten. Batch `mv` path covered by the same test.

## Shared closing tasks

- [ ] Changelog entries via `hyalo changelog add` (one per part).
- [ ] DEC-267 amendment (Part C) recorded in [[decision-log]].
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, all xtask `check-*` gates
      (invoke as `CARGO_MANIFEST_DIR=<repo>/crates/xtask ./target/debug/xtask <gate>`).

## Acceptance criteria

- [ ] BUG-3 fixture: `lint --fix` leaves it byte-identical; the six GitHub Docs odd-fence files
      get no MD031 proposal at the opener (`--dry-run`, read-only).
- [ ] BUG-28 fixture: `lint --fix` leaves it byte-identical; the autofixable-rule audit fixture
      is byte-identical after `--fix` and lives in the e2e suite; Outcome lists any other rule
      that needed the fix.
- [ ] BUG-4 on the mdn css copy: `--apply` produces only case-only diffs or none (per the
      recorded DEC-267 amendment); every dry-run `new_target` equals the applied `new_text`.
- [ ] BUG-7 fixture: nothing rewritten without `--allow-ambiguous`, four notes, all four
      rewritten with it; JSON records carry `property` for the frontmatter ones.
- [ ] Iteration 263's fixtures (`md034_ignores_urls_inside_link_destinations`,
      `md034_still_fires_on_a_bare_url_next_to_a_link`, `md034_ignores_urls_in_fenced_code_blocks`)
      and iteration 269's MD034/MD047 tests stay green.
- [ ] `../obsidian-hub` `lint --fix --dry-run` proposal delta by rule reported in the Outcome.
- [ ] Gates green; four changelog entries; one DEC amendment.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-261-270]] — BUG-3, BUG-28, BUG-4, BUG-7
- [[iterations/iteration-263-lint-autofix-obsidian-safety]] — the post-filter pattern
- [[iterations/iteration-269-mv-frontmatter-link-scan-gap]] — `plan_mv` widened scan
- [[decision-log]] — DEC-267, DEC-269, DEC-288
