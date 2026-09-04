---
type: iteration
title: Iteration 263 — Lint autofix safety on Obsidian content
date: 2026-09-03
status: completed
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

## Carry-over from iteration 262

[[iterations/iteration-262-frontmatter-wikilinks-first-class]] merged (PR #305)
with its own scope fully landed — every task and acceptance criterion ticked
against the real diff, nothing deferred inside the plan. Its PR review (local
`/review-pr` pass) found and fixed one issue in-PR (an `MvResult` text-output
shape gap) and surfaced two items that don't fit this plan's lint-autofix
scope, so neither is folded in here:

- The deeper finding — `mv`'s line-spanning-frontmatter-wikilink warning
  misses files with no other backlink to the target, plus a related
  pre-existing `NEW-3` ambiguous-link gap — is filed as its own plan,
  [[iterations/iteration-269-mv-frontmatter-link-scan-gap]] (next free slot),
  since it is `mv`/link-rewrite work, not lint autofix.
- Templated frontmatter (`created: {{date}}`) causing a file to be silently
  skipped, reconfirmed by iter-262's own dogfood outcome, was already tracked
  by this plan's sibling [[iterations/iteration-265-scan-exclude-and-skipped-files]]
  (SCAN-2) — no new item needed.

No change to this plan's own scope as a result.

## Tasks

### FIX-1: MD018 respects Obsidian tag grammar (BUG-3)

- [x] Define the rule precisely and record it as DEC-271 (tentative). Proposal:
      a line matching `^#+\S` is exempt from MD018 when the token after the
      hashes is a valid Obsidian tag, meaning it consists of letters, digits,
      `_`, `-`, `/` or non-ASCII word characters, contains at least one
      non-digit, and uses a single `#` (a `##tag` line is not a tag in
      Obsidian). MD018 keeps firing on `#1 item` (digits only), `#` followed by
      punctuation, and on `##Heading` with two or more hashes.
- [x] hyalo-mdlint MD018: implement the exemption in both detection and
      autofix; add a shared `is_obsidian_tag_token` helper so a future tag rule
      reuses it.
- [x] Unit tests: `#todo`, `#todo/next`, `#2024-goals`, `#日本語`, `#1`,
      `#!bang`, `##todo`, `#todo trailing text` (still a tag line in
      Obsidian), plus the two lines from `T - Thecookiemomma's Daily Log.md`
      quoted in the report.
- [x] e2e in `crates/hyalo-cli/tests/e2e`: `lint --fix --rule MD018` on a file
      with `#todo` leaves it byte-identical and reports no fix.
- [x] Docs: `hyalo lint-rules show MD018` description mentions the exemption;
      changelog entry.

### FIX-2: MD034 and MD042 understand link destinations and image link text (BUG-9)

- [x] hyalo-mdlint MD034: skip any URL that sits inside a markdown link or
      image destination `](…)`, an autolink `<…>`, a reference definition, or a
      wikilink; only a URL in plain prose is bare. Use the same span map the
      auto-linker uses for inert regions if one exists in hyalo-core.
- [x] hyalo-mdlint MD042: link text consisting of an image `![…](…)` or
      `![[…]]` is not empty. Keep flagging `[](url)` and `[ ](url)`.
- [x] Unit tests for both rules with `[![](img.png)](https://x/y.png)`, a bare
      URL after a link on the same line (must still fire), and a URL in a
      fenced code block (must not).
- [x] e2e: `lint --fix --fix-rule MD034` on the report's fixture line makes no
      change; `lint --rule MD042` reports 0 on it.
- [x] Docs: rule descriptions, changelog.

### FIX-3: MD001 stops autofixing deliberate heading jumps (UX-10)

- [x] DEC-272 (tentative): choose between (a) MD001 stays enabled but is no
      longer autofixable (report only), or (b) MD001 default-disabled in the
      built-in rule table. Recommend (a): a skipped heading level is worth a
      warning, but rewriting `######` to `##` changes what the author meant,
      and the fix is trivially applied by hand. Keep `lint-rules set MD001
      --enabled false` as the per-vault opt-out.
- [x] Implement in hyalo-mdlint (drop the fixer, keep the check) and update the
      built-in defaults table; `hyalo lint-rules list` shows `AUTOFIX no` for
      MD001.
- [x] e2e: `lint --fix --dry-run` on a `######` caption file proposes nothing
      for MD001 but `lint` still reports it as a warning.
- [x] Docs: `hyalo lint-rules show MD001`, `lint --help` autofix list, the
      `hyalo-knowledgebase/` lint docs page, changelog.

### FIX-4: explain autofix conflicts in text mode (UX-16)

- [x] hyalo-cli `lint --fix` text output: after `conflicts N`, print one line
      per conflict in the form `conflict  <file>  <rule> line <n>: range
      overlap with <other rule>` (the same data JSON already carries), capped
      at 20 lines with `… and N more (use --detailed)`. `--detailed` prints
      all.
- [x] e2e: a fixture producing an MD012/MD047 overlap shows the explanation
      line in text and unchanged JSON.
- [x] Docs: `lint -h/--help` output section, changelog.

## Acceptance criteria

- [x] Obsidian Hub, cwd `../obsidian-hub`, clean checkout: before the change
      `hyalo lint --fix --dry-run --format json --jq '[.results.fixes[] |
      .rule] | group_by(.) | map({(.[0]): length}) | add'` records the per-rule
      baseline (MD018 162, MD034 209, MD001 17 per the report); after the
      change MD018 proposals on tag lines are 0 (verify with `--rule MD018
      --detailed` that every remaining proposal is a real `#Heading` typo),
      MD034 proposals on link destinations are 0, MD001 proposals are 0.
- [x] `../obsidian-hub`: `hyalo lint --rule MD042 --count` → 0 for the
      image-as-link-text pattern (was 55); any remaining MD042 hit is a
      genuine `[](…)`.
- [x] `../obsidian-hub`: `hyalo lint --fix --fix-rule MD034 "02 - Community
      Expansions/02.05 All Community Expansions/CSS Snippets/Embed
      Adjustments.md"` followed by `git diff --exit-code` → exit 0.
- [x] `../obsidian-hub`: `hyalo lint --fix --dry-run --format text | grep -A3
      conflicts` shows a per-conflict explanation line.
- [x] Scratch vault: body line `#todo`, `hyalo lint --fix --rule MD018 <file>`
      → file unchanged, `fixed 0`; body line `#Heading typo` → still fixed.
- [x] `hyalo lint-rules list --format json --jq '.results[] |
      select(.id=="MD001") | .autofixable'` → `false`.
- [x] Own KB: `hyalo lint --strict` result unchanged; `hyalo lint --fix
      --dry-run --count` unchanged or lower, with any difference explained.
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`, `hyalo lint --strict` on the KB,
      xtask help-drift check.
- [x] Changelog entry via `hyalo changelog add`; DEC-271 and DEC-272 recorded
      in [[decision-log]]; `.claude/skills/hyalo/SKILL.md` lint paragraph
      updated where it lists autofixable rules.

## Outcome (2026-09-04)

Landed on `iter-263/lint-autofix-obsidian-safety`. Measured on `../obsidian-hub`
(6,520 files) with the release binary:

| Rule | `--fix --dry-run` proposals before | after |
|------|-----------------------------------|-------|
| MD018 | 162 | **0** |
| MD034 | 209 | **116** (all remaining are prose URLs) |
| MD001 | 17 | **0** |
| MD042 "Found empty link" | 55 | **0** |

Three deviations from the plan as written, each recorded in the decision log:

- **MD018 needed a tiebreak the plan did not anticipate.** `#todo trailing
  text` (exempt per the plan) and `#Heading typo` (still fixed per the
  acceptance criteria) are the same grammatical shape — a tag token followed by
  prose. DEC-271 resolves it by capitalization: a plain capitalized ASCII word
  followed by more text stays flagged. This retired the old
  `md018_ignores_paragraph_continuation_lines` assertion that `#standalone` is a
  heading; the test now uses `#Standalone typo`.
- **MD034 could not reuse `hyalo_core::links::inert_link_zones`.** That map
  closes a link label at the *first* `]`, so on `[![](img)](url)` the
  destination falls outside every zone — exactly the BUG-9 shape. `hyalo-mdlint`
  grew its own nesting-aware `link_markup_spans` instead; the auto-linker is
  unaffected because a bare URL is inert there on its own.
- **MD042 emits two diagnostics for one badge**, "Found empty link" on the link
  and "Found image with empty alt text" on the image. Both are suppressed when
  the image opens a link label, which is what gets the vault's count to 0 for
  the pattern. A standalone `![](img.png)` keeps its empty-alt warning — 21 of
  those remain on the Hub, in 6 files, and they are genuine.

Two things beyond the plan's four items, both forced by FIX-4:

- The iter-213 UX-5 dedup (never show a rule as both `fixed` and `conflict`)
  suppressed *every* conflict line in the MD012/MD047 fixture, which is why
  UX-16 saw `conflicts 2` and nothing else. Now that a conflict carries its
  line, the dedup is narrowed to the same *violation* (rule + line); a fixed
  group that lists no violations still suppresses its rule wholesale.
- `fix_mode_file_totals` counted distinct conflicting *rules* per file while the
  listing prints one line per violation, so the Hub showed 11 `conflict` lines
  under `conflicts 9`. Both now count `(rule, line)`.

Noticed while verifying, out of scope, worth a dogfood item: MD034's URL
boundary scan swallows a following `<br` in HTML-ish prose
(`Themes/Retroma.md:65`), so its autofix would emit
`<https://github.com/emarpiee/Retroma<br>`. Upstream boundary bug, three
occurrences vault-wide.

## Links

- [[dogfood-results/dogfood-v0220-obsidian-vaults]]
- [[iterations/iteration-265-scan-exclude-and-skipped-files]]
- [[decision-log]]
