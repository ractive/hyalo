---
title: Iteration 205 — frequency trigger for the common-title note
type: iteration
date: 2026-08-18
status: planned
branch: iter-205/common-title-frequency-trigger
tags:
  - iteration
  - links
  - auto-link
related:
  - "[[dogfood-results/dogfood-v0210-pre-release-iters-191-198]]"
  - "[[iterations/iteration-197-auto-link-stopword-heuristic]]"
---

# Iteration 205 — frequency trigger for the common-title note

## Goal

Close the dogfood's UX-1: the iter-197 note's wordlist trigger misses the
titles that actually dominate a run. On GitHub Docs, a page titled
"Workflows" produced 531 of 1,324 proposed links (40%) and was never
mentioned, while the note flagged "limits" (3.5%). Add a frequency/share
trigger alongside the wordlist; it is language-independent, so it also
fixes the ASCII gate that leaves non-English vaults with no warning at
all. Fold in the two LOW note refinements (L-12 truncation honesty, L-13
display casing).

**Do NOT release; release is a separate user-gated step.**

## Context

Numbers and repros:
[[dogfood-results/dogfood-v0210-pre-release-iters-191-198]] (UX-1, L-12,
L-13). Anchors at `931a226`: trigger + note assembly in
`crates/hyalo-cli/src/commands/links.rs` (~line 516 area), wordlist gate
`is_common_word` in `hyalo-core/src/common_words.rs:842`. The iter-197
plumbing (stderr-only, byte-identical stdout, self-extinguishing,
opt-outs) is verified correct and must not change — this iteration only
widens WHAT triggers the note.

## Tasks

- [ ] Design the trigger and record it in the plan before coding (a DEC
      entry): a candidate title is flagged when wordlist-common OR
      frequency-dominant. Starting point to validate against the
      report's data: matches >= max(15, 20% of total proposed links),
      computed per title from the same report the note already uses.
      Must flag Workflows (531/1324), metrics (59), runner groups (44),
      concurrency (40) on the GitHub Docs slice while staying SILENT on
      the own KB (192 candidates, 22 titles — currently zero notes, keep
      it that way; tune thresholds against both corpora and record the
      chosen values + measured outcomes here).
- [ ] Non-ASCII titles participate in the frequency path (drop the ASCII
      gate for frequency; keep it for the wordlist, whose entries are
      ASCII by construction).
- [ ] Note wording distinguishes the reason: "common English words" vs
      "unusually frequent" (or a merged phrasing naming both) — the
      user's judgment differs per cause.
- [ ] L-12: when offenders exceed the listing cap, say so ("showing the
      5 noisiest of 7") AND include --exclude-title flags for ALL
      offenders (flags are cheap; the cap is for the prose list only) so
      one paste-back fully extinguishes.
- [ ] L-13: display the most frequent original casing ("README (3x)")
      while matching stays case-insensitive; the suggested flag value
      keeps working either way.
- [ ] Frequency titles containing spaces/quotes now make the
      shell-quoting path (hints::shell_quote, #232 review fix) live —
      add an e2e proving a multi-word frequent title round-trips through
      the suggested flag ("runner groups" is the real-data case).
- [ ] e2e: GitHub Docs-shaped fixture where the dominant title is NOT a
      wordlist word; own-KB-shaped fixture asserting silence; stdout
      byte-identity re-asserted for the frequency path; opt-outs cover
      both trigger kinds.
- [ ] Docs: links auto --help, configuration.md ([links.auto]
      warn_common_titles covers both triggers — confirm the key name
      still fits or add a separate key ONLY if user-configurable
      thresholds are demanded; default: no new config).
- [ ] CHANGELOG entry.

## Acceptance criteria

- [ ] The report's GitHub Docs scenario names Workflows first with its
      count/share; pasting the suggestion extinguishes the note in one
      round
- [ ] Own KB stays note-free
- [ ] A German-titled scratch vault with one dominant title gets the
      note (ASCII gate no longer total)
- [ ] stdout remains byte-identical with/without the note in both
      formats
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Per-vault configurable thresholds (wait for demand — DEC it).
- Any change to match/exclusion semantics of links auto itself
  (iter-200 owns the apply-path fixes).
- iteration-199 (exclude-list counter-flags) stays deferred.
