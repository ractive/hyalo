---
type: iteration
title: "Iteration 284 — Stability retrospective (read-only analysis)"
date: 2026-09-06
status: completed
tags: [iteration, retrospective, architecture, analysis]
branch: iter-284/stability-retrospective
priority: 1
related:
  - "[[dogfood-results/dogfood-v0220-post-batch-261-270]]"
  - "[[dogfood-results/dogfood-v0220-post-batch-271-274]]"
  - "[[decision-log]]"
  - "[[iterations/iteration-285-typed-output-structs]]"
  - "[[dogfood-results/dogfood-v0220-obsidian-vaults]]"
  - "[[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]]"
---

# Iteration 284 — Stability retrospective (read-only analysis)

## Goal

Requested by James on 2026-09-06, mid-run of iteration 281, and deferred until that run had
merged: pause feature work and answer why every dogfood round finds many bugs and why every fix
adds features that add more bugs. This iteration writes **no code**. Its deliverable is a report
under `dogfood-results/` (or `research/`) with counts and citations, plus whatever decision-log
entries the findings justify. Explicitly **not** in scope: rebuilding an acceptance-criteria
machinery.

The fuzzy-scorer chain 279 → 280 → 281 → 282, each fixing the previous one's side effect, is one
data point; DEC-318 (summary vs find edge predicate) and DEC-293 (frontmatter fence parsing) are
two more.

## Tasks

- [x] TASK-1: **Bug triage.** For every finding in `dogfood-v0220-post-batch-261-270`,
      `dogfood-v0220-post-batch-271-274`, the Obsidian-vaults dogfood and the help-efficiency /
      find-shape dogfood, classify: real defect vs edge case vs spec preference; regression vs
      latent; which iteration introduced it (git blame the fix's test). Table with counts.
- [x] TASK-2: **Surface growth.** Measure the CLI/config surface at `v0.18.0`, `v0.21.0` and
      `main`: subcommand enum variants, `#[arg(` count in `crates/hyalo-cli/src/cli/args.rs`,
      `.hyalo.toml` keys, `--fields` values, `find` filter operators. Include the per-batch
      "breaking defaults" lists from the decision log. Read-only: `git show <tag>:<path>`.
- [x] TASK-3: **Defect classes.** Test the hypothesis that the recurring pattern is *one concept
      computed in more than one code path, then diverging*: disk scan vs `--index`, `summary` vs
      `find` predicates (DEC-318), `find` vs `mv` resolution, text vs JSON rendering, frontmatter
      fence parsing (DEC-293), ad-hoc `json!` envelopes vs typed structs (see
      [[iterations/iteration-285-typed-output-structs]]). For each class: how many triaged bugs
      it explains, and what single change would have caught them earlier (a shared function, a
      parity test, a gate).
- [x] TASK-4: Synthesize into one report: counts, the top three defect classes, what to stop
      doing, what to add (at most a handful of concrete, cheap guards). Recommend which of the
      queued iterations should run before 0.22.0 is released and which after. Run from
      `origin/main`, three research agents in parallel (triage / surface / classes), reports
      written to files, not returned inline.
- [x] TASK-5: `hyalo lint --strict` on the report; DEC entries for any rule the report
      establishes (e.g. "one concept, one function, parity-tested").

## Acceptance criteria

- [x] Every dogfood finding from the four named reports has a classification row.
- [x] Surface numbers exist for all three points (v0.18.0, v0.21.0, main) and are reproducible
      from the commands recorded in the report.
- [x] The report names the defect classes with the bug count each explains, and each class has
      one concrete prevention proposal.
- [x] No source file under `crates/` is modified on this branch.
- [x] Gates green (knowledgebase lint only).

## Outcome — 2026-09-06

[[research/stability-retrospective-2026-09-06]] records all 232 source observations:
138 defects, 18 edge/data/process observations and 76 preferences/extensions. Counts
are observations, not unique root causes. Historical attribution remains explicit:
94 precise defect introductions and 25 regression-versus-latent ages are unknown.
The report embeds reproducible three-revision surface measurements and finding ledgers.

Three research tracks ran from baseline `5f3a9128`; DEC-328 records four existing guard
families to extend. Independent read-only review found no actionable defects. Changed-file
strict Hyalo lint and whitespace checks pass; no Rust source, build or tests were needed.
The report's release recommendations do not change the owner's execution queue.

Upcoming plans 282, 283, 285, 286, 287 and the deferred 288 were reconciled. Plans 285
and 287 correct existing error-contract assumptions; 285 also incorporates the report's
limits on what typed structs prove. Other outcomes and acceptance criteria remain intact.
External publishing/platform prerequisites remain required, and 288 desktop verification
remains deferred.

## Links

- [[decision-log]] — DEC-293, DEC-318, DEC-324–326
- [[iterations/iteration-285-typed-output-structs]] — the first cleanup the analysis is expected
  to confirm
