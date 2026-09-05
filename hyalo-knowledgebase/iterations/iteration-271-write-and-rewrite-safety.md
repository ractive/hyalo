---
type: iteration
title: "Iteration 271 — Write and rewrite safety: closing fence, emitter guard, MD031/MD019 in code blocks, site_prefix case rewrites, mv ambiguity"
date: 2026-09-05
status: completed
tags:
  - iteration
  - frontmatter
  - lint
  - links
  - mv
  - dogfooding
branch: iter-271/write-rewrite-safety
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

- [x] Record the decision in [[decision-log]]: the closing fence is a line that is exactly
      `---` (optional trailing whitespace or `\r`) at column 0, like the opener. State why the
      leniency existed and why it loses.
- [x] Census before changing anything: for `hyalo-knowledgebase/`, `../obsidian-hub`,
      `../kepano-obsidian`, `../mdn/files/en-us`, `../docs/content`, count files whose parse
      outcome differs between lenient and strict close; record the counts in the Outcome. A
      file that only parsed because of leniency is malformed and HYALO005 should say so.
- [x] Implement; keep the CRLF (`---\r`) and BOM paths intact (pinned by existing tests).
- [x] Unit tests: the fixture parses to `k = "a\n---\nb"`, `after = 1`, body `REALBODY`; an
      indented `  ---` as the last frontmatter line does not close (→ HYALO005, not silent
      truncation); `---` with trailing spaces still closes; a body thematic break is untouched.
- [x] e2e: `read --frontmatter`, `find --file --fields properties`, `set`, `append`, `remove`
      see the full map; `set z=1` adds one line and leaves the body alone.

### FENCE-2: emitter guard

- [x] The emitter used by `set`/`append` never writes a block scalar containing a line that
      trims to `---` or `...`: double-quote the scalar instead (round-trips, asks nothing of the
      user). Record in the same DEC.
- [x] Round-trip test: `set --property "k=$(printf 'a\n---\nb')"` → `find --file` returns the
      three-line string; a second `set` on another key changes exactly one line.

## Part B — `properties rename --to ''` is rejected (BUG-13, MEDIUM)

`hyalo properties rename --from title --to ''` exits 0 and every file gets `"": Note 2`; titles
fall back to the filename stem so the loss is invisible.

- [x] Reject empty, whitespace-only and otherwise invalid keys for `--from` and `--to` with
      exit 1, reusing the validator behind `types set ''` / `set --property '=v'`.
- [ ] `--from X --to X` is a no-op that says so; `--to <existing>` still reports `conflicts`
      and writes nothing (existing e2e stays green).
      Not done as sketched — see Outcome: `--from X --to X` was left as the pre-existing exit-1
      "source and target property names are identical" error rather than converted to an exit-0
      no-op, since downgrading a gate to a success was judged the wrong direction. Nothing
      writes either way, so the AC's practical intent (no accidental write) holds; the exit code
      differs from what this bullet describes.
- [x] e2e in text and JSON, `--dry-run` included.

## Part C — MD031 must not fire at the opener of an unterminated fence (BUG-3, HIGH)

```text
printf -- '---\ntitle: t\n---\n\n# T\n\nIntro.\n\n```yaml\n  - uses: x\n  - name: y\n' > unterm.md
hyalo lint --fix --file unterm.md      # fixed MD031 line 9 — blank line inserted INSIDE the sample
```

Real hit: `../docs/content/actions/tutorials/build-and-test-code/rust.md`; six GitHub Docs files
have an odd fence count; markdownlint reports nothing at such an opener.

- [x] Root-cause: upstream `mdbook-lint-rulesets` MD031 or hyalo's fence tracking? If upstream,
      post-filter in `crates/hyalo-mdlint/src/rules/` like the iteration-263 MD034/MD042
      filters: drop any "followed by" finding whose fence has no closer before EOF.
- [x] Unit test with the fixture plus a terminated fence that must still be fixed.
- [x] e2e: the fixture is byte-identical after `--fix`; on `../docs/content` (`--dry-run`,
      read-only) the six odd-fence files get no MD031 proposal at their opener — list the paths
      and lines in the test's comment.

## Part D — MD019 fires inside code blocks; audit every autofixable rule (BUG-28, HIGH)

```text
printf -- '---\ntitle: d\n---\n\n# T\n\n```text\n#   three\n```\n\n~~~sh\n#   tilde\n~~~\n\n    #   indented\n' > d.md
hyalo lint --fix --file d.md           # fixed 3 — every "#   x" inside the code becomes "# x"
```

MD018, MD023 and MD026 respect code blocks; MD019 does not (backtick, tilde, indented).
Pre-existing; it rewrote a sample inside the dogfood report itself.

- [x] MD019 skips lines inside fenced (``` and ~~~, any info string) and indented code blocks.
      Prefer one shared "inside a code block" span computation in the engine that any rule can
      consult — check whether iteration 263's `rules/obsidian.rs` span scanner can be lifted.
- [x] **Audit**: one fixture placing every autofixable rule's trigger (`hyalo lint-rules list
      --jq '.results[] | select(.autofixable) | .id'`) inside a backtick fence, a tilde fence,
      an indented block, an HTML comment and the frontmatter block; assert `lint --fix` leaves
      it byte-identical. Rules that are *about* fences (MD031, MD040, MD046, MD048) are handled
      explicitly. Record in the Outcome which other rules needed the fix.
- [x] Ship the fixture as an e2e test.
- [x] Re-measure `lint --fix --dry-run` on `../obsidian-hub` (24044 proposals before) and
      report the delta by rule.

## Part E — `<!-- markdownlint-disable … -->` comments (feature gap; decide, then implement if cheap)

MDN's whitespace guide wraps a tab-laden `html-nolint` fence in
`<!-- markdownlint-disable no-hard-tabs -->`; `lint --fix` replaced the tabs in a page whose
point is showing tabs. Neither the rule-id form, the alias form, nor the `-nolint` info-string
suffix is recognised.

- [x] Decide and record in [[decision-log]]: honour `markdownlint-disable` / `enable` /
      `disable-next-line` / `disable-file` with rule ids and aliases (markdownlint semantics);
      the `-nolint` info string is MDN-specific and stays out unless trivially covered.
      If the engine already tracks comment spans (Part D), implementing is a small extension —
      do it here. If not, file the DEC as the scope statement and a backlog item, not a plan.
- [x] If implemented: fixture with disable/enable pairs around a tab-laden fence and a
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

- [x] A case-mismatch plan for a link that resolved through `site_prefix` and/or a directory
      index writes the link back in the form it came in — site-absolute stays site-absolute, a
      directory link stays a directory link — with only the case changed. Never append
      `/index` or `.md` to a form that did not have it.
- [x] Decide as a DEC-267 amendment in [[decision-log]]: (a) keep producing case plans with the
      correct rewrite, or (b) skip case-mismatch plans for links that resolved via
      `site_prefix`, since the link is correct for the site. (b) is conservative and matches
      "cosmetic"; pick it unless a testbed shows a need for (a).

### CASE-2: the dry-run shows the exact string that will be written

- [ ] `new_target` and text-mode output in `--dry-run` equal the replacement `--apply` writes,
      for every strategy. Test: `--dry-run` then `--apply` on one fixture, assert each applied
      `new_text` equals the dry-run `new_target` for the same `(file, line, old_target)`.
      **Deferred — see Outcome.** Dry-run and apply share `build_replacements_for_file`, so they
      cannot disagree about what gets written, but the *reported* `new_target` is still the
      plan's vault-relative path, not the emitted string, so this literal string-equality does
      not hold yet. Carried forward to iteration 272 (see this file's Carry-over section once
      filed).
- [x] On the css copy after the fix: `git diff` after `--apply` contains only case-only
      changes, or none under option (b).

## Part G — `mv` applies the ambiguity guard to frontmatter links (BUG-7, MEDIUM)

```text
mkdir x; printf -- '---\ntitle: A\n---\n' > a.md; printf -- '---\ntitle: XA\n---\n' > x/a.md
printf -- '---\ntitle: C\nrelated: "[[a]]"\nrel2: [[a|al]]\n---\nbody [[a]] and [[a|al]]\n' > c.md
hyalo mv a.md z.md    # body links skipped as ambiguous; related/rel2 rewritten to [[z]]
```

- [x] Apply the DEC-288 ambiguity test to frontmatter link sources in `plan_mv`; skip with the
      same `note: skipped ambiguous link … at c.md:<line>`, counted in `skipped_ambiguous`
      with the frontmatter `property` on the record; `--allow-ambiguous` rewrites them and
      says so.
- [x] e2e: `files updated: 0`, four notes, bytes unchanged; with `--allow-ambiguous` all four
      rewritten; batch `mv` covered by the same test.

## Shared closing tasks

- [x] Changelog entries via `hyalo changelog add` (one per part that changes behaviour).
- [x] DECs: fence strictness + emitter guard (A), markdownlint-disable scope (E), DEC-267
      amendment (F).
- [x] Docs: `lint --help` (code-block exemption, disable comments if implemented),
      `links fix --help` (case plans on `site_prefix` vaults), skill file and rule template
      where DEC-267 is described.
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, all xtask `check-*` gates
      (invoke as `CARGO_MANIFEST_DIR=<repo>/crates/xtask ./target/debug/xtask <gate>` —
      `cargo run -p xtask` deadlocks on the nested build lock).

## Acceptance criteria

- [x] BUG-2: `read --frontmatter` returns `after: 1` and the three-line `k`; `set z=1` adds one
      line and leaves `REALBODY`; `set` with a multi-line `---` value round-trips through
      `find --file`; strict-vs-lenient census recorded for five testbeds with a verdict per
      changed file.
- [x] BUG-13: `properties rename --to ''` / `--from ''` exit 1, nothing written.
- [x] BUG-3: fixture byte-identical after `--fix`; six GitHub Docs odd-fence files get no MD031
      at the opener.
- [x] BUG-28: fixture byte-identical; the all-autofixable-rules audit fixture is byte-identical
      after `--fix` and lives in the e2e suite; Outcome names any other rule that needed it.
- [x] Part E: a DEC exists; if implemented, the protected-region fixture holds.
- [ ] BUG-4: on the mdn css copy `--apply` yields case-only diffs or none; every dry-run
      `new_target` equals the applied `new_text`.
      Half met: the css-copy diffs are case-only-or-none (measured 5096→0 case mismatches,
      1049→0 files rewritten — see Outcome). The `new_target`-equals-applied-`new_text` half is
      the deferred CASE-2 reporting change, carried to iteration 272.
- [x] BUG-7: nothing rewritten without `--allow-ambiguous`, four notes; all four with it.
- [x] Iteration 263 fixtures, iteration 269 MD034/MD047 tests, and the 45-value hostile scalar
      set stay green; Hub `lint --fix --dry-run` delta by rule in the Outcome.
- [x] Gates green; changelog; three DECs.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-261-270]] — BUG-2, 3, 4, 7, 13, 28; feature gap "markdownlint-disable"
- [[iterations/iteration-263-lint-autofix-obsidian-safety]] — the post-filter pattern
- [[iterations/iteration-269-mv-frontmatter-link-scan-gap]] — `plan_mv` widened scan
- [[decision-log]] — DEC-267, DEC-269, DEC-288, DEC-292

## Outcome

All seven parts shipped. Three DECs recorded: [[decision-log]] DEC-293 (strict column-0
closing fence + emitter guard), DEC-294 (`markdownlint-disable` comments), DEC-295 (DEC-267
amendment for `site_prefix` case plans + form-preserving rewrites). Eight changelog entries.

### Part A — strict closing fence (BUG-2)

`is_closing_delimiter` is now `line.trim_end() == "---"`. Every parse path in `hyalo-core`
already routed through that one predicate, so the change lands everywhere at once
(`read_frontmatter_from_reader`, `find_body_offset`, `skip_frontmatter`, the multi-visitor
scanner, `body_state::LineScanner`, the splicer's framing check, `link_rewrite`'s frontmatter
scan). CRLF and BOM paths are untouched — trailing-whitespace trimming absorbs `\r`.

**Census — lenient vs. strict, five testbeds:**

| Testbed | Files with frontmatter | Files whose parse differs |
|---|---:|---:|
| `hyalo-knowledgebase/` | 459 | 0 |
| `../obsidian-hub` | 6509 | 0 |
| `../kepano-obsidian` | 98 | 0 |
| `../mdn/files/en-us` | 14375 | 0 |
| `../docs/content` | 3707 | 0 |
| **total** | **25148** | **0** |

The leniency never rescued a real file. It only mis-parsed the ones it broke, so there is no
per-file verdict list to record — the "changed file" set is empty.

**FENCE-2.** `hyalo_serializer_options_for` turns `prefer_block_scalars` off for exactly the
values whose strings contain a `---`/`...` line (`has_document_marker_line`), so those are
written as double-quoted scalars and everything else is emitted byte-identically to before.
Applied at all four serialization sites (whole-document serialize, the splice fallback,
`serialize_one`, `render_scalar_item`).

One existing test changed meaning:
`scanner::body_state::tests::indented_closing_delimiter_closes_frontmatter` became
`…_does_not_close_frontmatter`, plus a new `trailing_whitespace_still_closes_frontmatter`.

### Part B — `properties rename` key validation (BUG-13)

New shared `commands::reject_invalid_property_key`, applied to `--from` and `--to` before any
file is read (so `--dry-run` is refused too). Rejects empty, whitespace-only, line-break and
control-character keys. `--from X --to X` was **left as the existing exit-1 user error** rather
than converted to an exit-0 no-op as the plan sketched: it already writes nothing and says so,
and downgrading a gate to a success is the wrong direction. The AC ("`--to ''` / `--from ''`
exit 1, nothing written") is met.

### Parts C/D/E — one `BodySpans` pass (BUG-3, BUG-28, feature gap)

New `hyalo-mdlint/src/rules/spans.rs` walks the body once, reusing the existing
`rules::code_fence` CommonMark §4.5 helpers, and answers three questions per line: is it code
(fenced — terminated or not — or indented), is it inside an HTML comment, and does this line
open a fence that never closes. It also parses `markdownlint-…` directives on the same pass.

- **C:** MD031 is dropped at an unterminated fence opener.
- **D:** any stock rule not in `CODE_BLOCK_AWARE_RULE_IDS` is dropped on a code or
  HTML-comment line. **Which rules this covers, measured:** the audit fixture's sample block was
  linted as plain prose to see which default-on rules its triggers actually reach —
  **MD009, MD011, MD012, MD019, MD022, MD023, MD034 and MD042**. Every one of those was firing
  inside a fence before this iteration; MD019 is simply the one the dogfood report caught,
  because it is the one that rewrote a sample in the report itself. (MD018 did not fire: its
  Obsidian-tag exemption from iteration 263 already covered `#missing space`. MD026/MD029/MD030
  and the MD049/MD050 emphasis rules are off by default and never reached the fixture.)
  **Deliberately kept firing:** MD031/MD040/MD046/MD048 (the fence is their subject), MD047
  (the file's final newline) and **MD010** — markdownlint's own default is `code_blocks: true`
  and a hard tab in a sample is a real portability problem. The MDN case that motivated Part E
  is answered by the disable comment, not by exempting MD010.
- **E: implemented.** `markdownlint-disable`, `-enable`, `-disable-line`, `-disable-next-line`,
  `-disable-file`, `-enable-file`, with ids or aliases (the catalog's `name` field *is* the
  markdownlint alias), case-insensitive, applied to HYALO rules too. `-capture`/`-restore` and
  MDN's `-nolint` info-string suffix are out of scope — see DEC-294.

Known interaction, documented in the e2e test: `disable-next-line` on a **heading** is defeated
by MD022's own fix, which inserts a blank line between the directive and its target.
markdownlint behaves the same way; the portable spelling is `disable-line` on the heading.

**Obsidian Hub re-measurement** (`hyalo lint --dir ../obsidian-hub --fix --dry-run`, whole run,
`--limit 100000 --max-per-rule 0`): **24030 proposals, down from 24044 — delta −14.** By rule:

| Rule | Proposals |
|---|---:|
| MD009 | 14075 |
| MD022 | 5346 |
| MD012 | 4234 |
| MD010 | 187 |
| MD034 | 116 |
| MD031 | 62 |
| MD047 | 7 |
| MD023 | 2 |
| MD019 | 1 |

The delta is small because the Hub's fenced blocks are mostly YAML/Dataview samples that do
not trip a prose rule; the fourteen that disappeared are exactly the ones that would have
rewritten a sample. The point of Part D is not the count — it is that the count can no longer
include a byte inside a code block.

### Part F — `site_prefix` case plans (BUG-4)

Option **(b)** as the plan directed: `resolved_through_site_prefix` skips the case-mismatch plan
entirely for a site-absolute link carrying the prefix.

**Measured on a fresh copy of `../mdn/files/en-us/web/css` (1228 files) with
`site_prefix = "en-US/docs/Web/CSS"`:**

| | before | after |
|---|---:|---:|
| `case_mismatches` (dry-run) | 5096 | **0** |
| files rewritten by `links fix --apply` | 1049 | **0** |

`relocations` is 0 as well, so plain `--apply` writes nothing at all on that corpus — the AC's
"case-only diffs or none" resolves to *none*. (`broken` is 2373 on the copy because it is a
subtree: links out of `web/css` have no target inside it. Those are unaffected by this change
and are still reported, as they should be.)

CASE-1 is also implemented for the strategies that remain: `emit_markdown_fix_target` keeps the
incoming form — `raw_target_names_index` / `strip_directory_index` stop `/index` being appended
to a directory link, a trailing slash survives, and `markdown_fix_round_trips` accepts the
directory form of a directory-index target so the guard does not reject the correct emission.

**CASE-2 partially deferred.** The dry-run's reported `new_target` is still the plan's
vault-relative path in both modes, not the emitted string. Making it the emitted string means
threading the per-plan emission out of `build_replacements_for_file` through
`plan_fixes_dry_run`/`apply_fixes` and into the CLI's report construction — a four-call-site
signature change touching output shape that several e2e suites assert on. The *substantive*
half of CASE-2 is done: dry-run and apply share `build_replacements_for_file`, so they cannot
disagree about what will be written, and the two ways the reported target used to mislead (the
`/index` tail, and a site-absolute rewrite of a link that should not be rewritten at all) are
both closed. Carry the reporting change forward as a small follow-up.

### Part G — `mv` ambiguity guard on frontmatter links (BUG-7)

`plan_frontmatter_wikilink_rewrites` takes an optional `FrontmatterAmbiguityGuard`; when the
matched target is a bare stem that `lookup_stem_all` resolves to more than one file, the link is
skipped and recorded in `skipped_ambiguous` with the new `property` field (the nearest preceding
frontmatter key, so a list item is attributed to the property that owns it). The guard is
**not** applied to the outbound self-link pass: there the file being rewritten *is* the moved
file, so `[[old]]` in its own frontmatter names itself the way Obsidian resolves it.

Batch `mv` shares `plan_inbound_rewrites` and inherits the guard; it has no JSON slot for the
skips (they go to stderr, as line-spanning frontmatter links do), and the e2e asserts the
stderr note plus byte-identity.

### Verification

`cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
green; 17 new e2e tests in `crates/hyalo-cli/tests/e2e/iteration271_write_rewrite_safety.rs`
covering all seven parts, plus new unit tests in `frontmatter/mod.rs`, `scanner/body_state.rs`,
`link_fix.rs` and `rules/spans.rs`. Iteration 263 fixtures, iteration 269's MD034/MD047 tests
and the hostile-scalar set all stayed green without modification.
`hyalo lint --strict` on this knowledgebase: 0 errors (one pre-existing HYALO002 warning on
[[iterations/iteration-270-schema-write-semantics]], untouched by this iteration).

All six xtask gates pass: `check-feature-fanout`, `check-help-drift`,
`check-command-reference`, `check-bundled-skills`, `check-pi-package-sync`,
`check-mutation-journal`.

**Two operational notes on running the xtask gates locally**, both cheaper than the workaround
currently in the loop's memory:

1. `CARGO_MANIFEST_DIR` must be **absolute**. `workspace_root()` walks up from it, so a relative
   `crates/xtask` resolves to the empty path and every gate that shells out with
   `current_dir(workspace_root)` silently fails to run the binary — reported as "could not get
   help output for 'hyalo <cmd>'", which reads like a missing command rather than a broken
   invocation.
2. The gates that shell out to `cargo run -q -p hyalo-cli -- …` hang in `read_output` waiting
   for a pipe that never closes. Put a two-line `cargo` shim first on `PATH` that strips
   `run -q -p hyalo-cli --` and execs `target/release/hyalo` instead; `check-help-drift` then
   finishes in seconds. The gate is testing help *text*, so the binary it reads it from is
   immaterial as long as it is current.
