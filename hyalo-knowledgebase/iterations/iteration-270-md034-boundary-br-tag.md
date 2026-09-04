---
type: iteration
title: "Iteration 270 — MD034: URL boundary scan must stop before a following HTML tag"
date: 2026-09-04
status: planned
tags:
  - iteration
  - lint
  - obsidian
  - dogfooding
branch: iter-270/md034-boundary-br-tag
priority: 10
related:
  - "[[iterations/iteration-263-lint-autofix-obsidian-safety]]"
  - "[[dogfood-results/dogfood-v0220-obsidian-vaults]]"
---

# Iteration 270 — MD034: URL boundary scan must stop before a following HTML tag

## Goal

Carry-over from [[iterations/iteration-263-lint-autofix-obsidian-safety]] (found
while verifying that iteration's MD034 fix, out of scope there since it is a
distinct upstream boundary bug rather than the Obsidian-tag-line or
link-destination corruption that iteration targeted): MD034's bare-URL
end-of-match boundary scan does not stop at a following HTML tag. On
`../obsidian-hub`, `Themes/Retroma.md:65` has a bare URL immediately followed
by `<br` (an unclosed/self-closing HTML line break used inline in prose), and
`hyalo lint --fix --fix-rule MD034` on that line would wrap it as
`<https://github.com/emarpiee/Retroma<br>` — swallowing the `<br` into the
angle-bracket autolink and corrupting the markup, one more variant of the
same "MD034's autofix rewrites something it should have left alone" failure
class iteration 263 closed for the link-destination and reference-definition
cases. Three occurrences vault-wide on the Hub as of iter-263's dogfood run.

Constraint: **no new CLI flags** from dogfood pressure (project rule). This is
a boundary-detection fix inside the existing MD034 post-processing added in
iteration 263
(`crates/hyalo-mdlint/src/rules/obsidian.rs`), not a new flag or rule. Out of
scope: any other MD034 boundary case not yet observed in the corpus — fix the
`<tag` case this iteration and note in the outcome whether the same scan
mismeasures a boundary against other adjacent punctuation (e.g. `>`, `)`
outside a link) so a follow-up can be filed if the corpus turns up more.

## Tasks

### FIX-1: stop the bare-URL span before a following HTML tag

- [ ] Confirm the repro: reduce `Themes/Retroma.md:65` (or an equivalent
      fixture) to a minimal line — bare URL immediately followed by `<br` (no
      space) — and show `lint --fix --dry-run --rule MD034` proposes wrapping
      the `<br` into the autolink.
- [ ] Root-cause in upstream `mdbook-lint-rulesets`' MD034 URL-boundary scan
      (the same rule iteration 263 already post-processes, not a fork): does
      it stop at whitespace/EOL only, missing `<` as a boundary character? If
      the boundary bug is upstream-only and not reachable through hyalo's own
      `rules::obsidian` code, the fix belongs in `rules::obsidian` as another
      post-filter (like the iteration-263 ones), narrowing the span hyalo
      accepts from the diagnostic rather than patching upstream.
- [ ] Implement: MD034's proposed URL end boundary must not extend past a
      literal `<` that starts an HTML tag (self-closing or not) immediately
      following the URL, so `https://…/Retroma<br>` fixes to
      `<https://…/Retroma><br>` (or, if simpler and still correct, the fix is
      suppressed entirely for this shape and the diagnostic still fires as a
      warning per iter-263's own bias toward under-fixing over corrupting).
- [ ] Unit test in `rules::obsidian` (or wherever the fix lands) for
      `https://a.example/<br>`, `https://a.example/<br/>`, and a bare URL
      immediately followed by `>` alone (confirm that shape, if it exists in
      the corpus, is unaffected or also handled) plus the existing
      already-fixed shapes from iteration 263 (regression guard).
- [ ] e2e in `crates/hyalo-cli/tests/e2e`: the reduced Retroma-style fixture,
      `lint --fix --fix-rule MD034`, assert the file either keeps the `<br`
      outside the autolink or is left unchanged with the diagnostic still
      reported.
- [ ] Docs: update the MD034 description suffix
      (`crates/hyalo-mdlint/src/engine.rs::DESCRIPTION_SUFFIX`) and
      `hyalo-knowledgebase/docs/schema-and-lint.md`'s Obsidian-grammar table if
      the fix changes what MD034 is documented to skip; changelog entry.

## Acceptance criteria

- [ ] `../obsidian-hub`: `hyalo lint --fix --dry-run --rule MD034 --format
      json --jq '[.results.fixes[] | select(.file | test("Retroma"))]'` no
      longer proposes a fix that would embed `<br` inside the autolink's angle
      brackets (proposes nothing for that line, or proposes a fix whose result
      keeps `<br` outside the brackets — pick one and match it in the e2e
      test).
- [ ] `../obsidian-hub`: `git diff --exit-code` after `hyalo lint --fix
      --fix-rule MD034` on the three known-affected files (or the whole vault)
      → either exit 0 (fix suppressed) or a diff that leaves `<br` intact
      outside the wrapped URL.
- [ ] Every other MD034 fixture from iteration 263
      (`md034_ignores_urls_inside_link_destinations`,
      `md034_still_fires_on_a_bare_url_next_to_a_link`,
      `md034_ignores_urls_in_fenced_code_blocks`) still passes unchanged —
      this iteration narrows a boundary, it does not touch the iter-263
      exemptions.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`, `hyalo lint --strict` on the KB,
      xtask help-drift check.
- [ ] Changelog entry via `hyalo changelog add`; a DEC recorded in
      [[decision-log]] if the fix is non-obvious (e.g. suppress-vs-narrow
      choice).

## Links

- [[iterations/iteration-263-lint-autofix-obsidian-safety]]
- [[dogfood-results/dogfood-v0220-obsidian-vaults]]
- [[decision-log]]
