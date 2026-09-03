---
type: iteration
title: Iteration 263 — Lint autofix safety on Obsidian content
date: 2026-09-03
status: planned
tags:
  - iteration
  - lint
  - obsidian
  - dogfooding
branch: iter-263/lint-autofix-obsidian-safety
priority: 3
related:
  - "[[dogfood-results/dogfood-v0220-obsidian-vaults]]"
---

# Iteration 263 — Lint autofix safety on Obsidian content

## Goal

`hyalo lint --fix` must never corrupt an Obsidian vault. The report
[[dogfood-results/dogfood-v0220-obsidian-vaults]] found three autofixes that
do: MD018 turns a line-start tag such as `#todo` into the heading `# todo`
(BUG-3, 162 proposals on Obsidian Hub), MD034 wraps a URL that is already a
link destination in angle brackets and MD042 calls `[![](img)](url)` an empty
link (BUG-9, 209 and 55 hits), and MD001 flattens deliberate `######` captions
on CSS-snippet notes to `##` (UX-10, 17 hits). A fourth item is the text
output of `lint --fix`, which prints `conflicts 2` with no explanation (UX-16).
The work lives almost entirely in hyalo-mdlint; hyalo-cli only changes the
conflict reporting.

Constraint: **no new CLI flags** from dogfood pressure (project rule). MD001's
change is a rule default managed through `hyalo lint-rules`, not a flag;
conflict details go into the existing text output and the existing
`--detailed`. Out of scope: frontmatter parsing diagnostics and `[lint] ignore`
semantics ([[iterations/iteration-265-scan-exclude-and-skipped-files]]),
the lint-ignore override for named files
([[iterations/iteration-267-help-hints-text-polish]]), and any new MD rule.

## Tasks

### FIX-1: MD018 respects Obsidian tag grammar (BUG-3)

- [ ] Define the rule precisely and record it as DEC-271 (tentative). Proposal:
      a line matching `^#+\S` is exempt from MD018 when the token after the
      hashes is a valid Obsidian tag, meaning it consists of letters, digits,
      `_`, `-`, `/` or non-ASCII word characters, contains at least one
      non-digit, and uses a single `#` (a `##tag` line is not a tag in
      Obsidian). MD018 keeps firing on `#1 item` (digits only), `#` followed by
      punctuation, and on `##Heading` with two or more hashes.
- [ ] hyalo-mdlint MD018: implement the exemption in both detection and
      autofix; add a shared `is_obsidian_tag_token` helper so a future tag rule
      reuses it.
- [ ] Unit tests: `#todo`, `#todo/next`, `#2024-goals`, `#日本語`, `#1`,
      `#!bang`, `##todo`, `#todo trailing text` (still a tag line in
      Obsidian), plus the two lines from `T - Thecookiemomma's Daily Log.md`
      quoted in the report.
- [ ] e2e in `crates/hyalo-cli/tests/e2e`: `lint --fix --rule MD018` on a file
      with `#todo` leaves it byte-identical and reports no fix.
- [ ] Docs: `hyalo lint-rules show MD018` description mentions the exemption;
      changelog entry.

### FIX-2: MD034 and MD042 understand link destinations and image link text (BUG-9)

- [ ] hyalo-mdlint MD034: skip any URL that sits inside a markdown link or
      image destination `](…)`, an autolink `<…>`, a reference definition, or a
      wikilink; only a URL in plain prose is bare. Use the same span map the
      auto-linker uses for inert regions if one exists in hyalo-core.
- [ ] hyalo-mdlint MD042: link text consisting of an image `![…](…)` or
      `![[…]]` is not empty. Keep flagging `[](url)` and `[ ](url)`.
- [ ] Unit tests for both rules with `[![](img.png)](https://x/y.png)`, a bare
      URL after a link on the same line (must still fire), and a URL in a
      fenced code block (must not).
- [ ] e2e: `lint --fix --fix-rule MD034` on the report's fixture line makes no
      change; `lint --rule MD042` reports 0 on it.
- [ ] Docs: rule descriptions, changelog.

### FIX-3: MD001 stops autofixing deliberate heading jumps (UX-10)

- [ ] DEC-272 (tentative): choose between (a) MD001 stays enabled but is no
      longer autofixable (report only), or (b) MD001 default-disabled in the
      built-in rule table. Recommend (a): a skipped heading level is worth a
      warning, but rewriting `######` to `##` changes what the author meant,
      and the fix is trivially applied by hand. Keep `lint-rules set MD001
      --enabled false` as the per-vault opt-out.
- [ ] Implement in hyalo-mdlint (drop the fixer, keep the check) and update the
      built-in defaults table; `hyalo lint-rules list` shows `AUTOFIX no` for
      MD001.
- [ ] e2e: `lint --fix --dry-run` on a `######` caption file proposes nothing
      for MD001 but `lint` still reports it as a warning.
- [ ] Docs: `hyalo lint-rules show MD001`, `lint --help` autofix list, the
      `hyalo-knowledgebase/` lint docs page, changelog.

### FIX-4: explain autofix conflicts in text mode (UX-16)

- [ ] hyalo-cli `lint --fix` text output: after `conflicts N`, print one line
      per conflict in the form `conflict  <file>  <rule> line <n>: range
      overlap with <other rule>` (the same data JSON already carries), capped
      at 20 lines with `… and N more (use --detailed)`. `--detailed` prints
      all.
- [ ] e2e: a fixture producing an MD012/MD047 overlap shows the explanation
      line in text and unchanged JSON.
- [ ] Docs: `lint -h/--help` output section, changelog.

## Acceptance criteria

- [ ] Obsidian Hub, cwd `../obsidian-hub`, clean checkout: before the change
      `hyalo lint --fix --dry-run --format json --jq '[.results.fixes[] |
      .rule] | group_by(.) | map({(.[0]): length}) | add'` records the per-rule
      baseline (MD018 162, MD034 209, MD001 17 per the report); after the
      change MD018 proposals on tag lines are 0 (verify with `--rule MD018
      --detailed` that every remaining proposal is a real `#Heading` typo),
      MD034 proposals on link destinations are 0, MD001 proposals are 0.
- [ ] `../obsidian-hub`: `hyalo lint --rule MD042 --count` → 0 for the
      image-as-link-text pattern (was 55); any remaining MD042 hit is a
      genuine `[](…)`.
- [ ] `../obsidian-hub`: `hyalo lint --fix --fix-rule MD034 "02 - Community
      Expansions/02.05 All Community Expansions/CSS Snippets/Embed
      Adjustments.md"` followed by `git diff --exit-code` → exit 0.
- [ ] `../obsidian-hub`: `hyalo lint --fix --dry-run --format text | grep -A3
      conflicts` shows a per-conflict explanation line.
- [ ] Scratch vault: body line `#todo`, `hyalo lint --fix --rule MD018 <file>`
      → file unchanged, `fixed 0`; body line `#Heading typo` → still fixed.
- [ ] `hyalo lint-rules list --format json --jq '.results[] |
      select(.id=="MD001") | .autofixable'` → `false`.
- [ ] Own KB: `hyalo lint --strict` result unchanged; `hyalo lint --fix
      --dry-run --count` unchanged or lower, with any difference explained.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`, `hyalo lint --strict` on the KB,
      xtask help-drift check.
- [ ] Changelog entry via `hyalo changelog add`; DEC-271 and DEC-272 recorded
      in [[decision-log]]; `.claude/skills/hyalo/SKILL.md` lint paragraph
      updated where it lists autofixable rules.

## Links

- [[dogfood-results/dogfood-v0220-obsidian-vaults]]
- [[iterations/iteration-265-scan-exclude-and-skipped-files]]
- [[decision-log]]
