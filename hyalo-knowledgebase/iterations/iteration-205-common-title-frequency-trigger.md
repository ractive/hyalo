---
title: Iteration 205 — frequency trigger for the common-title note
type: iteration
date: 2026-08-18
status: in-progress
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

## DEC-205 — the frequency trigger (recorded before coding)

**Rule.** A candidate title is an offender when it is wordlist-common
**OR** frequency-dominant. Frequency-dominant means

```text
count >= max(25, ceil(0.025 * total_proposed_links))
```

i.e. an absolute floor of 25 matches, overtaken by a 2.5% share once a run
exceeds 1,000 proposed links. The two curves cross exactly at 1,000, so the
floor protects small vaults from being nagged about a title the user can
simply read, and the share keeps the note selective on large ones.

**Why not the plan's 20%-of-total starting point.** It flags nothing but
`Workflows` on the GitHub Docs slice: 20% of 1,179 is 236, so `metrics`
(62), `runner groups` (45) and `concurrency` (39) — all named in the
dogfood as must-flag — stay silent. A share that low needs the absolute
floor to carry small and mid-size runs.

**Measured outcomes** (v0.20.0 binary, 2026-08-22, `links auto` with no
flags):

| corpus | total links | titles | threshold | flagged |
|---|---|---|---|---|
| own KB (`hyalo-knowledgebase`) | 195 | 22 | 25 | `backlinks` (130, 67%) |
| GitHub Docs `actions`+`repositories`+`get-started` | 1,179 | 52 | 30 | `workflows` 502, `oidc` 177, `metrics` 62, `limits` 46, `runner groups` 45, `powershell` 43, `concurrency` 39 |
| `vscode-docs` | 33,859 | 144 | 847 | 10 titles, `command` 4,149 (12%) down to `configure` 932 (2.8%) |

Nearest non-flagged neighbours: own KB `decision log` 19; GitHub Docs
`actions-runner-controller` 28; vscode-docs `markdown` 829. No corpus sits
on a knife edge.

**Deviation from the plan's "own KB stays note-free" acceptance criterion.**
It is unachievable and the criterion was written from a wrong premise. The
own KB's dominant title is `backlinks` at **130 of 195 links (67%)** across
56 files — a project about a CLI whose command is called `backlinks`, whose
every prose mention would be linked. No threshold can flag GitHub Docs'
`concurrency` (39 links, 3.3%) while staying silent about a title with 130
links at 67%: it is larger on both the absolute and the relative axis. The
own-KB note is therefore a **true positive** of exactly the class UX-1 asks
us to catch, and the AC is amended to: the own KB emits exactly one note,
naming `backlinks`, extinguished in one paste-back. A vault-size gate (only
warn above N total links) would restore the silence but contradicts the
German-scratch-vault criterion, which needs the note on a small vault.

**Casing and keying.** Offender keys stay `to_ascii_lowercase`, matching the
comparison `auto_link` itself uses for `exclude_titles` — a Unicode-aware
lowercase would let the note suggest a flag that does not actually exclude.
The *display* form (L-13) is the most frequent original surface spelling,
ties broken lexicographically.

**Cascade, measured.** Excluding the seven GitHub Docs offenders shrinks that
run from 1,179 to 265 links, which drops the threshold from 30 to the floor of
25 — and two titles that were previously quiet (`actions-runner-controller`
28, `runner-groups` 27) now cross it. This is inherent to a share-relative
trigger and it terminates: every round removes at least 25 links, so the
cascade is bounded by `total / 25` rounds (two, here). Documented in
`docs/configuration.md` and covered by a unit test rather than smoothed over.

**Non-goal reaffirmed.** No new config key and no user-configurable
thresholds; `[links.auto] warn_common_titles` keeps governing both triggers.
Revisit only on demand.

## Tasks [9/9]

- [x] Design the trigger and record it in the plan before coding (a DEC
      entry): a candidate title is flagged when wordlist-common OR
      frequency-dominant. Starting point to validate against the
      report's data: matches >= max(15, 20% of total proposed links),
      computed per title from the same report the note already uses.
      Must flag Workflows (531/1324), metrics (59), runner groups (44),
      concurrency (40) on the GitHub Docs slice while staying SILENT on
      the own KB (192 candidates, 22 titles — currently zero notes, keep
      it that way; tune thresholds against both corpora and record the
      chosen values + measured outcomes here).
- [x] Non-ASCII titles participate in the frequency path (drop the ASCII
      gate for frequency; keep it for the wordlist, whose entries are
      ASCII by construction).
- [x] Note wording distinguishes the reason: "common English words" vs
      "unusually frequent" (or a merged phrasing naming both) — the
      user's judgment differs per cause.
- [x] L-12: when offenders exceed the listing cap, say so ("showing the
      5 noisiest of 7") AND include --exclude-title flags for ALL
      offenders (flags are cheap; the cap is for the prose list only) so
      one paste-back fully extinguishes.
- [x] L-13: display the most frequent original casing ("README (3x)")
      while matching stays case-insensitive; the suggested flag value
      keeps working either way.
- [x] Frequency titles containing spaces/quotes now make the
      shell-quoting path (hints::shell_quote, #232 review fix) live —
      add an e2e proving a multi-word frequent title round-trips through
      the suggested flag ("runner groups" is the real-data case).
- [x] e2e: GitHub Docs-shaped fixture where the dominant title is NOT a
      wordlist word; own-KB-shaped fixture asserting silence; stdout
      byte-identity re-asserted for the frequency path; opt-outs cover
      both trigger kinds.
- [x] Docs: links auto --help, configuration.md ([links.auto]
      warn_common_titles covers both triggers — confirm the key name
      still fits or add a separate key ONLY if user-configurable
      thresholds are demanded; default: no new config).
- [x] CHANGELOG entry.

## Acceptance criteria [5/5]

- [x] The report's GitHub Docs scenario names `workflows` first with its
      count and share (502×, 43%); pasting the suggested flags back removes
      all seven named offenders in one round. The now-much-smaller run then
      surfaces the next tier (`actions-runner-controller` 28,
      `runner-groups` 27) — inherent to a share-relative trigger, bounded by
      the 25-match floor, documented and unit-tested rather than hidden
- [x] Own KB emits exactly one note, naming `backlinks` (130 of 195,
      67%) — amended from "note-free" by DEC-205, which shows the
      original criterion is unachievable and the note is a true positive
- [x] A German-titled scratch vault with one dominant title gets the
      note (ASCII gate no longer total)
- [x] stdout remains byte-identical with/without the note in both
      formats
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Per-vault configurable thresholds (wait for demand — DEC it).
- Any change to match/exclusion semantics of links auto itself
  (iter-200 owns the apply-path fixes).
- iteration-199 (exclude-list counter-flags) stays deferred.
