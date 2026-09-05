---
type: iteration
title: "Iteration 271 — Write and rewrite safety: closing fence, emitter guard, MD031/MD019 in code blocks, site_prefix case rewrites, mv ambiguity"
date: 2026-09-05
status: planned
tags:
  - iteration
  - frontmatter
  - lint
  - links
  - mv
  - dogfooding
branch: iter-271/write-and-rewrite-safety
priority: 1
related:
  - "[[dogfood-results/dogfood-v0220-post-batch-261-270]]"
  - "[[iterations/iteration-263-lint-autofix-obsidian-safety]]"
  - "[[iterations/iteration-269-mv-frontmatter-link-scan-gap]]"
  - "[[decision-log]]"
---

# Iteration 271 — Write and rewrite safety

## Goal

Every case in [[dogfood-results/dogfood-v0220-post-batch-261-270]] where a hyalo *mutation*
touched bytes it should have left alone and exited 0: two in the frontmatter writer, three in
`lint --fix`, one in `links fix --apply`, one in `mv`. Groups 1 and 2 of the report's
recommendations merged into one iteration (per-iteration overhead is ~90 min regardless of
size; group 1 alone was two fixes). Parts A–B and F live in `hyalo-core`, C–E in
`hyalo-mdlint`; each has a fixture from the report. BUG-1 (concurrent writers) is closed
won't-fix in DEC-292 and out of scope.

Constraint: **no new CLI flags**. Every fix narrows what an existing mutation touches or makes
an existing dry-run honest.

## Part A — an indented `  ---` inside the frontmatter is not a closing fence (BUG-2, HIGH)

`is_closing_delimiter` in `crates/hyalo-core/src/frontmatter/parse.rs` accepts any line whose
trimmed text is `---`, documented in a comment as deliberate leniency. No DEC covers it; YAML
and Obsidian close only at column 0.

```text
printf -- '---\ntitle: Ind\nk: |-\n  a\n  ---\n  b\nafter: 1\n---\nREALBODY\n' > ind.md
hyalo read ind.md --frontmatter --jq '.results.frontmatter'   # {"k":"a","title":"Ind"} — after: lost
hyalo set ind.md --property z=1                                # replaces "  ---" with "z: 1" + "---"
```

hyalo produces the trigger itself: `set --property "k=$(printf 'a\n---\nb')"` emits `k: |-`
with an indented `---`, and `find --file` then reads `k: "a"`; the next mutation destroys the
block. `lint` reports nothing.

### FENCE-1: strict column-0 close

- [ ] Record the decision in [[decision-log]]: the closing fence is a line that is exactly
      `---` (optional trailing whitespace or `\r`) at column 0, like the opener. State why the
      leniency existed and why it loses.
- [ ] Census before changing anything: for `hyalo-knowledgebase/`, `../obsidian-hub`,
      `../kepano-obsidian`, `../mdn/files/en-us`, `../docs/content`, count files whose parse
      outcome differs between lenient and strict close; record the counts in the Outcome. A
      file that only parsed because of leniency is malformed and HYALO005 should say so.
- [ ] Implement; keep the CRLF (`---\r`) and BOM paths intact (pinned by existing tests).
- [ ] Unit tests: the fixture parses to `k = "a\n---\nb"`, `after = 1`, body `REALBODY`; an
      indented `  ---` as the last frontmatter line does not close (→ HYALO005, not silent
      truncation); `---` with trailing spaces still closes; a body thematic break is untouched.
- [ ] e2e: `read --frontmatter`, `find --file --fields properties`, `set`, `append`, `remove`
      see the full map; `set z=1` adds one line and leaves the body alone.

### FENCE-2: emitter guard

- [ ] The emitter used by `set`/`append` never writes a block scalar containing a line that
      trims to `---` or `...`: double-quote the scalar instead (round-trips, asks nothing of the
      user). Record in the same DEC.
- [ ] Round-trip test: `set --property "k=$(printf 'a\n---\nb')"` → `find --file` returns the
      three-line string; a second `set` on another key changes exactly one line.

## Part B — `properties rename --to ''` is rejected (BUG-13, MEDIUM)

`hyalo properties rename --from title --to ''` exits 0 and every file gets `"": Note 2`; titles
fall back to the filename stem so the loss is invisible.

- [ ] Reject empty, whitespace-only and otherwise invalid keys for `--from` and `--to` with
      exit 1, reusing the validator behind `types set ''` / `set --property '=v'`.
- [ ] `--from X --to X` is a no-op that says so; `--to <existing>` still reports `conflicts`
      and writes nothing (existing e2e stays green).
- [ ] e2e in text and JSON, `--dry-run` included.

## Part C — MD031 must not fire at the opener of an unterminated fence (BUG-3, HIGH)

```text
printf -- '---\ntitle: t\n---\n\n# T\n\nIntro.\n\n```yaml\n  - uses: x\n  - name: y\n' > unterm.md
hyalo lint --fix --file unterm.md      # fixed MD031 line 9 — blank line inserted INSIDE the sample
```

Real hit: `../docs/content/actions/tutorials/build-and-test-code/rust.md`; six GitHub Docs files
have an odd fence count; markdownlint reports nothing at such an opener.

- [ ] Root-cause: upstream `mdbook-lint-rulesets` MD031 or hyalo's fence tracking? If upstream,
      post-filter in `crates/hyalo-mdlint/src/rules/` like the iteration-263 MD034/MD042
      filters: drop any "followed by" finding whose fence has no closer before EOF.
- [ ] Unit test with the fixture plus a terminated fence that must still be fixed.
- [ ] e2e: the fixture is byte-identical after `--fix`; on `../docs/content` (`--dry-run`,
      read-only) the six odd-fence files get no MD031 proposal at their opener — list the paths
      and lines in the test's comment.

## Part D — MD019 fires inside code blocks; audit every autofixable rule (BUG-28, HIGH)

```text
printf -- '---\ntitle: d\n---\n\n# T\n\n```text\n#   three\n```\n\n~~~sh\n#   tilde\n~~~\n\n    #   indented\n' > d.md
hyalo lint --fix --file d.md           # fixed 3 — every "#   x" inside the code becomes "# x"
```

MD018, MD023 and MD026 respect code blocks; MD019 does not (backtick, tilde, indented).
Pre-existing; it rewrote a sample inside the dogfood report itself.

- [ ] MD019 skips lines inside fenced (``` and ~~~, any info string) and indented code blocks.
      Prefer one shared "inside a code block" span computation in the engine that any rule can
      consult — check whether iteration 263's `rules/obsidian.rs` span scanner can be lifted.
- [ ] **Audit**: one fixture placing every autofixable rule's trigger (`hyalo lint-rules list
      --jq '.results[] | select(.autofixable) | .id'`) inside a backtick fence, a tilde fence,
      an indented block, an HTML comment and the frontmatter block; assert `lint --fix` leaves
      it byte-identical. Rules that are *about* fences (MD031, MD040, MD046, MD048) are handled
      explicitly. Record in the Outcome which other rules needed the fix.
- [ ] Ship the fixture as an e2e test.
- [ ] Re-measure `lint --fix --dry-run` on `../obsidian-hub` (24044 proposals before) and
      report the delta by rule.

## Part E — `<!-- markdownlint-disable … -->` comments (feature gap; decide, then implement if cheap)

MDN's whitespace guide wraps a tab-laden `html-nolint` fence in
`<!-- markdownlint-disable no-hard-tabs -->`; `lint --fix` replaced the tabs in a page whose
point is showing tabs. Neither the rule-id form, the alias form, nor the `-nolint` info-string
suffix is recognised.

- [ ] Decide and record in [[decision-log]]: honour `markdownlint-disable` / `enable` /
      `disable-next-line` / `disable-file` with rule ids and aliases (markdownlint semantics);
      the `-nolint` info string is MDN-specific and stays out unless trivially covered.
      If the engine already tracks comment spans (Part D), implementing is a small extension —
      do it here. If not, file the DEC as the scope statement and a backlog item, not a plan.
- [ ] If implemented: fixture with disable/enable pairs around a tab-laden fence and a
      `disable-next-line`; `lint --fix` leaves the protected region byte-identical; `lint`
      reports nothing there and still reports outside it.

## Part F — `links fix` case-mismatch rewrites on a `site_prefix` vault (BUG-4, HIGH)

On a copy of `../mdn/files/en-us/web/css` with `site_prefix = "en-US/docs/Web/CSS"`:

```text
hyalo links fix --dry-run --jq '.results.case_mismatch_fixes[0]'
# old_target "/en-US/docs/Web/CSS/Guides/Anchor_positioning", new_target "guides/anchor_positioning/index.md"
hyalo links fix --apply    # 5096 links in 1049 files → /en-US/docs/Web/CSS/guides/anchor_positioning/index
```

The written URL carries a trailing `/index`; the dry-run shows vault-relative while apply
writes site-absolute; DEC-267 calls the rule cosmetic while it rewrote 5096 links in a corpus
whose URL convention is Title-case over lowercase folders.

### CASE-1: the rewrite keeps the incoming form

- [ ] A case-mismatch plan for a link that resolved through `site_prefix` and/or a directory
      index writes the link back in the form it came in — site-absolute stays site-absolute, a
      directory link stays a directory link — with only the case changed. Never append
      `/index` or `.md` to a form that did not have it.
- [ ] Decide as a DEC-267 amendment in [[decision-log]]: (a) keep producing case plans with the
      correct rewrite, or (b) skip case-mismatch plans for links that resolved via
      `site_prefix`, since the link is correct for the site. (b) is conservative and matches
      "cosmetic"; pick it unless a testbed shows a need for (a).

### CASE-2: the dry-run shows the exact string that will be written

- [ ] `new_target` and text-mode output in `--dry-run` equal the replacement `--apply` writes,
      for every strategy. Test: `--dry-run` then `--apply` on one fixture, assert each applied
      `new_text` equals the dry-run `new_target` for the same `(file, line, old_target)`.
- [ ] On the css copy after the fix: `git diff` after `--apply` contains only case-only
      changes, or none under option (b).

## Part G — `mv` applies the ambiguity guard to frontmatter links (BUG-7, MEDIUM)

```text
mkdir x; printf -- '---\ntitle: A\n---\n' > a.md; printf -- '---\ntitle: XA\n---\n' > x/a.md
printf -- '---\ntitle: C\nrelated: "[[a]]"\nrel2: [[a|al]]\n---\nbody [[a]] and [[a|al]]\n' > c.md
hyalo mv a.md z.md    # body links skipped as ambiguous; related/rel2 rewritten to [[z]]
```

- [ ] Apply the DEC-288 ambiguity test to frontmatter link sources in `plan_mv`; skip with the
      same `note: skipped ambiguous link … at c.md:<line>`, counted in `skipped_ambiguous`
      with the frontmatter `property` on the record; `--allow-ambiguous` rewrites them and
      says so.
- [ ] e2e: `files updated: 0`, four notes, bytes unchanged; with `--allow-ambiguous` all four
      rewritten; batch `mv` covered by the same test.

## Shared closing tasks

- [ ] Changelog entries via `hyalo changelog add` (one per part that changes behaviour).
- [ ] DECs: fence strictness + emitter guard (A), markdownlint-disable scope (E), DEC-267
      amendment (F).
- [ ] Docs: `lint --help` (code-block exemption, disable comments if implemented),
      `links fix --help` (case plans on `site_prefix` vaults), skill file and rule template
      where DEC-267 is described.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, all xtask `check-*` gates
      (invoke as `CARGO_MANIFEST_DIR=<repo>/crates/xtask ./target/debug/xtask <gate>` —
      `cargo run -p xtask` deadlocks on the nested build lock).

## Acceptance criteria

- [ ] BUG-2: `read --frontmatter` returns `after: 1` and the three-line `k`; `set z=1` adds one
      line and leaves `REALBODY`; `set` with a multi-line `---` value round-trips through
      `find --file`; strict-vs-lenient census recorded for five testbeds with a verdict per
      changed file.
- [ ] BUG-13: `properties rename --to ''` / `--from ''` exit 1, nothing written.
- [ ] BUG-3: fixture byte-identical after `--fix`; six GitHub Docs odd-fence files get no MD031
      at the opener.
- [ ] BUG-28: fixture byte-identical; the all-autofixable-rules audit fixture is byte-identical
      after `--fix` and lives in the e2e suite; Outcome names any other rule that needed it.
- [ ] Part E: a DEC exists; if implemented, the protected-region fixture holds.
- [ ] BUG-4: on the mdn css copy `--apply` yields case-only diffs or none; every dry-run
      `new_target` equals the applied `new_text`.
- [ ] BUG-7: nothing rewritten without `--allow-ambiguous`, four notes; all four with it.
- [ ] Iteration 263 fixtures, iteration 269 MD034/MD047 tests, and the 45-value hostile scalar
      set stay green; Hub `lint --fix --dry-run` delta by rule in the Outcome.
- [ ] Gates green; changelog; three DECs.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-261-270]] — BUG-2, 3, 4, 7, 13, 28; feature gap "markdownlint-disable"
- [[iterations/iteration-263-lint-autofix-obsidian-safety]] — the post-filter pattern
- [[iterations/iteration-269-mv-frontmatter-link-scan-gap]] — `plan_mv` widened scan
- [[decision-log]] — DEC-267, DEC-269, DEC-288, DEC-292
