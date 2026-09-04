---
type: iteration
title: "Iteration 269 — Carry-over correctness fixes: mv split-link scan, MD034 <tag boundary, MD047 on frontmatter-only files"
date: 2026-09-04
status: in-progress
tags:
  - iteration
  - mv
  - links
  - dogfooding
  - lint
branch: iter-269/mv-frontmatter-link-scan-gap
priority: 9
related:
  - "[[backlog/mv-frontmatter-split-link-detection-gap]]"
  - "[[iterations/iteration-262-frontmatter-wikilinks-first-class]]"
  - "[[iterations/iteration-263-lint-autofix-obsidian-safety]]"
  - "[[iterations/iteration-267-help-hints-text-polish]]"
---

# Iteration 269 — Carry-over correctness fixes: mv split-link scan, MD034 `<tag` boundary, MD047 on frontmatter-only files

## Goal

Three independent, fully-reproduced correctness bugs left over from the 261–268 batch,
bundled into one iteration because each is a contained fix with a known fixture and none
involves a design decision. Bundling them saves two full compile/test/review cycles. They
touch two crates (`hyalo-core` for SCAN, `hyalo-mdlint` for the two lint fixes) and no
shared code, so the three task groups can be implemented and tested independently and
landed as one PR.

Consolidated on 2026-09-04 from the original iteration-269 (mv scan gap), iteration-270
(MD034 `<br` boundary, carried over from [[iterations/iteration-263-lint-autofix-obsidian-safety]])
and iteration-271 (MD047 frontmatter-only file, carried over from
[[iterations/iteration-267-help-hints-text-polish]]). The original plan texts are in git
history at commit `99e20289` and its parents.

Constraint for all three: **no new CLI flags** from dogfood pressure (project rule). Every
fix here is detection- or boundary-completeness inside existing behaviour.

## Part A — mv: scan beyond the backlinks graph for split frontmatter links

### Background

Iteration 262 (FM-2) taught `hyalo mv` to warn when a frontmatter `[[…]]` spans a line
break — a folded (`>`) or literal (`|`) block scalar, or a wrapped quoted string —
instead of silently leaving it dangling. Its PR #305 review (2026-09-04) found the
warning only fires for a file `mv` already scans for another reason: `plan_mv`'s file
set (`by_source`, in `crates/hyalo-core/src/link_rewrite.rs`) comes from
`LinkGraph::backlinks_ci(old_rel)`, and the FM-1 graph scanner
(`extract_frontmatter_links`) never extracts a split-across-lines wikilink as a graph
edge in the first place. So a file whose *only* reference to the moved target is the
folded/wrapped link gets neither a rewrite nor a warning — exactly the
silent-dangling-reference failure mode FM-2 exists to close. Full repro and analysis in
[[backlog/mv-frontmatter-split-link-detection-gap]].

The same probing found a related, pre-existing gap in `NEW-3`'s ambiguous-bare-link
detection (`skipped_ambiguous`): two files sharing a stem where *neither* sits at the
vault root are never flagged as ambiguous by `mv`, and which of two same-stemmed
candidates gets flagged depends on which one is being moved. Both gaps stem from the same
root cause — `mv` only ever looks at files the backlinks graph already flagged.

Out of scope: any change to what counts as a graph edge for `backlinks`/`summary`/`--orphan`
(FM-1's narrower scope — a split-across-lines link is deliberately not counted there, and
this iteration must not change that).

### SCAN-1: widen `plan_mv`'s file set for split-link detection

- [ ] Decide the mechanism: either (a) `plan_mv` runs a cheap secondary pass over every
      vault file not already in `by_source`, gated on the frontmatter block containing an
      unclosed `[[` (skip immediately if it doesn't), reusing
      `split_frontmatter_wikilink`'s existing whitespace-collapsed substring test; or (b)
      `LinkGraph`/`FileLinks` gains a lightweight "unresolved split occurrence"
      side-channel populated during the normal build pass, consulted only by `plan_mv`.
      Record the choice as a DEC in [[decision-log]] — (a) is simpler and scoped to `mv`;
      (b) reuses work the build pass already does but couples an `mv`-only concern into
      the shared graph.
- [ ] Implement for single-file `mv`. Decide whether batch `mv` gets the same treatment in
      this iteration or a follow-up — batch already accepts a higher per-call cost, but the
      widened scan multiplies by every move in the batch.
- [ ] Fold `NEW-3`'s ambiguous-bare-link detection into the same widened scan (or record why
      not, if the mechanisms turn out not to share the necessary plumbing).
- [ ] Unit/e2e tests: a vault where a file's *only* reference to the moved target is a
      split frontmatter wikilink — `mv` reports it under `frontmatter_links_skipped` (JSON)
      and prints the stderr warning (text). A second fixture for the `NEW-3` case: two
      same-stemmed files, both nested, a bare-link reference to the ambiguous stem, and
      `mv` of either candidate reports `skipped_ambiguous` regardless of which one moves.
- [ ] Confirm unchanged: `backlinks`/`summary`/`--orphan` still do not treat a split
      frontmatter link as a graph edge.

### SCAN-2: perf check

- [ ] Measure `mv` on Obsidian Hub (6520 files, `../obsidian-hub`) with no split links
      present, before and after — must stay within noise of the iter-262 baseline
      (`summary` was 0.40 s median of 3; `mv` has no existing baseline, so establish one).
- [ ] If the widened scan shows up in the measurement, limit it to files whose frontmatter
      is already scanned for other reasons, or add a size/count guard with a documented
      tradeoff — do not add a new CLI flag to opt out.

## Part B — MD034: URL boundary scan must stop before a following HTML tag

### Background

Found while verifying iteration 263's MD034 fix, out of scope there since it is a
distinct upstream boundary bug: MD034's bare-URL end-of-match boundary scan does not
stop at a following HTML tag. On `../obsidian-hub`, `Themes/Retroma.md:65` has a bare URL
immediately followed by `<br`, and `hyalo lint --fix --fix-rule MD034` on that line would
wrap it as `<https://github.com/emarpiee/Retroma<br>` — swallowing the `<br` into the
angle-bracket autolink and corrupting the markup. Three occurrences vault-wide on the Hub
as of iter-263's dogfood run.

Out of scope: any other MD034 boundary case not yet observed in the corpus — fix the
`<tag` case and note in the outcome whether the same scan mismeasures a boundary against
other adjacent punctuation (e.g. `>`, `)` outside a link) so a follow-up can be filed.

### MD034-1: stop the bare-URL span before a following HTML tag

- [ ] Confirm the repro: reduce `Themes/Retroma.md:65` to a minimal line — bare URL
      immediately followed by `<br` (no space) — and show
      `lint --fix --dry-run --rule MD034` proposes wrapping the `<br` into the autolink.
- [ ] Root-cause in upstream `mdbook-lint-rulesets`' MD034 URL-boundary scan: does it stop
      at whitespace/EOL only, missing `<` as a boundary? If the bug is upstream-only, the
      fix belongs in `crates/hyalo-mdlint/src/rules/obsidian.rs` as another post-filter
      (like the iteration-263 ones), narrowing the span hyalo accepts from the diagnostic.
- [ ] Implement: MD034's proposed URL end boundary must not extend past a literal `<`
      that starts an HTML tag immediately following the URL, so `https://…/Retroma<br>`
      fixes to `<https://…/Retroma><br>` — or, if simpler and still correct, the fix is
      suppressed for this shape and the diagnostic still fires as a warning (iter-263's
      bias toward under-fixing over corrupting).
- [ ] Unit tests in `rules::obsidian` for `https://a.example/<br>`,
      `https://a.example/<br/>`, and a bare URL immediately followed by `>` alone, plus
      the existing iteration-263 shapes as a regression guard.
- [ ] e2e in `crates/hyalo-cli/tests/e2e`: the reduced Retroma-style fixture,
      `lint --fix --fix-rule MD034`, assert the file either keeps the `<br` outside the
      autolink or is left unchanged with the diagnostic still reported.
- [ ] Docs: update the MD034 description suffix
      (`crates/hyalo-mdlint/src/engine.rs::DESCRIPTION_SUFFIX`) and
      `hyalo-knowledgebase/docs/schema-and-lint.md`'s Obsidian-grammar table if the fix
      changes what MD034 is documented to skip.

## Part C — MD047 false positive on a frontmatter-only file

### Background

Found during iteration 267's `new --dry-run` work (PR #310 reviewer note): a file whose
content is **only** a frontmatter block — `---\n<yaml>\n---\n`, ending with a single
trailing `\n` — trips `MD047` ("File is missing a trailing newline"), even though the
file plainly ends with one. Reproduces on a hand-written file too, so it is a
pre-existing bug in hyalo's lint engine, not something iteration 267 introduced.

Confirmed repro (2026-09-04):

```text
$ printf -- '---\ntitle: X\ntype: note\n---\n' > vault/a.md
$ hyalo lint --file vault/a.md --jq '.results.files'
[{"file":"a.md", "rule_groups":[{"rule":"MD047","severity":"warn", ...,
  "violations":[{"message":"File is missing a trailing newline", ...}]}]}]
```

Most likely cause: the markdown BODY handed to the mdbook-lint engine after stripping
the frontmatter block is the empty string for a frontmatter-only file, and MD047's check
(or hyalo's own CRLF-handling wrapper around it — see `crates/hyalo-mdlint/src/engine.rs`
around the "MD047 must handle CRLF terminators" section) treats an empty body as
trivially "missing a trailing newline" rather than as "nothing to check".

### MD047-1: don't flag a frontmatter-only file as missing a trailing newline

- [ ] Confirm the repro with a minimal fixture (frontmatter-only, single trailing `\n`)
      and a second fixture (frontmatter-only, NO trailing newline after the closing
      `---`) to establish the two cases MD047 needs to tell apart — "empty body after
      valid frontmatter" must not fire; a truncated frontmatter block is a HYALO005/parse
      concern, not this rule's job either way.
- [ ] Root-cause in `crates/hyalo-mdlint/src/engine.rs`: hyalo's frontmatter-stripping
      (handing MD047 an empty body), hyalo's CRLF-handling MD047 wrapper, or upstream
      MD047 itself? The existing "single-line body carries no terminator to sample" guard
      suggests this edge-case class has been hit before for a different shape — check
      whether the same guard should extend to a zero-line body.
- [ ] Implement: an empty markdown body (frontmatter-only file, or a genuinely empty file)
      must not fire MD047. Prefer skipping the rule entirely for a 0-byte body over
      fabricating a diagnosis.
- [ ] Unit test in `crates/hyalo-mdlint/src/engine.rs` for a frontmatter-only body plus
      the pre-existing single-line-body test near it (regression guard).
- [ ] e2e in `crates/hyalo-cli/tests/e2e`: `hyalo new --type note --file x.md` followed
      by `hyalo lint --file x.md` reports no MD047 violation.
- [ ] Docs: `DESCRIPTION_SUFFIX` or the MD047 entry in
      `hyalo-knowledgebase/docs/schema-and-lint.md` if the fix changes what MD047 is
      documented to check.

## Shared closing tasks

- [ ] One changelog entry per part via `hyalo changelog add` (three entries).
- [ ] The SCAN-1 mechanism DEC recorded in [[decision-log]]; a DEC for MD034 or MD047 only
      if the chosen scope (suppress-vs-narrow, skip-when-empty vs. narrower) is non-obvious.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, all xtask `check-*`
      gates.

## Acceptance criteria

### Part A

- [ ] Reproduces then fixes the exact repro in
      [[backlog/mv-frontmatter-split-link-detection-gap]]: a file whose only reference to
      the moved target is a line-spanning frontmatter wikilink gets the warning.
- [ ] The related `NEW-3` gap (two nested same-stemmed files) is fixed or explicitly
      deferred with a recorded reason.
- [ ] `backlinks`/`summary`/`--orphan`/`find --dead-end` behaviour is byte-for-byte
      unchanged on both `../kepano-obsidian` and the own knowledgebase.
- [ ] Perf: `mv` on `../obsidian-hub` with no split links stays within noise of a
      newly-established baseline.

### Part B

- [ ] `../obsidian-hub`: `hyalo lint --fix --dry-run --rule MD034 --format json --jq
      '[.results.fixes[] | select(.file | test("Retroma"))]'` no longer proposes a fix
      that would embed `<br` inside the autolink's angle brackets.
- [ ] `../obsidian-hub`: after `hyalo lint --fix --fix-rule MD034` on the three
      known-affected files, `git diff` is either empty (fix suppressed) or leaves `<br`
      intact outside the wrapped URL.
- [ ] Every other MD034 fixture from iteration 263
      (`md034_ignores_urls_inside_link_destinations`,
      `md034_still_fires_on_a_bare_url_next_to_a_link`,
      `md034_ignores_urls_in_fenced_code_blocks`) still passes unchanged.

### Part C

- [ ] `hyalo new --type note --file notes/x.md && hyalo lint --file notes/x.md --jq
      '.results.violations'` → `0`, and the file's bytes are unchanged by `--fix`.
- [ ] A hand-written frontmatter-only file (`---\nkey: v\n---\n`) does not trigger
      MD047; a file whose closing fence itself lacks a trailing newline is unaffected by
      this fix (out of scope — a different, pre-existing shape).
- [ ] Existing MD047 fixtures (CRLF handling, single-line body, multi-fix convergence) in
      `crates/hyalo-mdlint/src/engine.rs` still pass unchanged.

### Whole iteration

- [ ] Gates green; three changelog entries; DECs recorded as described above.

## Links

- [[backlog/mv-frontmatter-split-link-detection-gap]]
- [[iterations/iteration-262-frontmatter-wikilinks-first-class]]
- [[iterations/iteration-263-lint-autofix-obsidian-safety]]
- [[iterations/iteration-267-help-hints-text-polish]]
- [[dogfood-results/dogfood-v0220-obsidian-vaults]]
- [[decision-log]]
