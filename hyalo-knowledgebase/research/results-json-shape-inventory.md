---
date: 2026-08-25
status: completed
tags:
- cli
- json
- output
- ux
- consistency
title: results JSON Shape Inventory (iter-216)
type: research
related:
- "[[iterations/iteration-216-results-shape-consistency]]"
- "[[iterations/iteration-213-config-ux-polish]]"
- "[[iterations/iteration-215-anchor-and-broken-links-followups]]"
- "[[research/cli-structured-output-patterns]]"
---

# results JSON Shape Inventory (iter-216)

Vault-wide survey of the `results` payload every command emits inside the
`{"results": …, "total": N, "hints": [...]}` envelope. Carries
[[iterations/iteration-213-config-ux-polish]]'s scoped-out non-goal
("Unifying `.results` JSON shapes across commands — needs its own design
pass") and the second half of dogfood **UX-6** that
[[iterations/iteration-215-anchor-and-broken-links-followups]] left unfiled
("`.results` JSON shape varying by command").

Method: the shapes below were read off `target/debug/hyalo <cmd> --format
json --no-hints` against a real vault (227 files) and a scratch vault, not
off the source, so what is recorded is what a script actually receives.

## Inventory

| Command | `results` type | Top-level keys |
| --- | --- | --- |
| `find` | array | per-file objects: `file`, `properties`, `tags`, `links`, `tasks`, … |
| `properties` (summary) | array | `{name, count, type}` |
| `tags` (summary) | array | `{name, count}` |
| `summary` | object | `files{total,directories[]}`, `properties[]`, `tags[]`, `links{total,broken}`, `orphans`, `dead_ends`, `tasks`, `schema{errors,warnings,files_with_issues}` |
| `config` | object | `dir`, `config_path`, `cwd`, `format`, `hints_enabled`, `site_prefix`, `site_prefix_source`, `malformed`, `parse_error`, `raw_contents`, `exempt`, `links_auto{…}`, `links_fuzzy_min_confidence`, `dir_overridden`, `dir_salvaged`, `dir_out_of_bounds`, `dir_out_of_bounds_reason`, `pi{…}` |
| `lint` | object | `files[]`, `total`, `errors`, `warnings`, `files_with_violations`, `files_checked`, `files_ignored`, `files_truncated`, `rules_fired`, (`dry_run`, `fixes`) |
| `lint --fix` | object | `files[]`, `total_fixed`, `total_remaining`, `total_conflicts`, `remaining_errors`, `remaining_warnings`, `files_with_violations`, `files_checked`, `files_truncated`, `rules_fired`, `dry_run` |
| `links` / `links fix` | object | `broken`, `fixable`/`fixes[]`, `fuzzy`/`fuzzy_fixes[]`, `case_mismatches`/`case_mismatch_fixes[]`, `relocations`/`relocation_fixes[]`, `unfixable`/`unfixable_links[]`, `ambiguous`/`ambiguous_links[]`, `out_of_vault`/`out_of_vault_links[]`, `templated`/`templated_links[]`, `applied`/`applied_fixes[]`, `failed`/`failed_fixes[]`, `unapplied`/`unapplied_fixes[]`, `ignored`, `broken_anchors`, `fuzzy_applied`, `fuzzy_below_floor`, `fuzzy_min_confidence` |
| `links auto` | object | `matches[]`, `total`, `scanned`, `applied`, `apply_outcomes[]`, `files_applied`, `files_failed`, `files_skipped`, `ambiguous_titles[]` |
| `set` / `append` | object (or array, one entry per `--property`/`--tag`) | `property`\|`tag`, `value`, `modified[]`, `skipped[]`, `scanned`, `total`, `dry_run` |
| `remove` | same, without `value` | `property`\|`tag`, `modified[]`, `skipped[]`, `scanned`, `total`, `dry_run` |
| `properties rename` | object | `from`, `to`, `modified[]`, `skipped_count`, `conflicts[]`, `scanned`, `total`, `dry_run` |
| `tags rename` | object | `from`, `to`, `modified[]`, `skipped_count`, `scanned`, `total`, `dry_run` |
| `mv` | object | `from`, `to`, `updated_files[]`, `total_files_updated`, `total_links_updated`, `dry_run` |
| `task toggle` | object | `file`, `line`, `text`, `status`, `done` |

## Rules derived from the inventory

These are the conventions the majority of commands already follow. They are
the yardstick used to classify each divergence below, and the contract new
commands should be held to.

- **R1 — `results.total` is a denominator.** The envelope owns `total`.
  When a command also puts `total` inside `results`, it must mean *the
  number of items the command considered*, never a count of findings.
  `set`/`remove`/`append`/`properties rename`/`tags rename` comply
  (`total = modified + skipped [+ conflicts]`).
- **R2 — top-level `results` keys are always present**, including when the
  value is `0`, `false`, `[]` or `null`. Per-item records *inside* arrays may
  omit optional keys, because per-item omission is a size optimization that
  costs nothing in jq (`.missing` is `null` anyway).
- **R3 — count/list pairing.** A list is named for the record type it holds
  (`…_fixes` for fix proposals, `…_links` for links). A count is either the
  bare bucket name or `<bucket>_count`. Where the list would be unbounded and
  uninformative, emit only the count.
- **R4 — one concept, one key name across commands.**

## Classification

### Fixed — accidental drift

| ID | Finding | Rule | Change | Breaking? |
| --- | --- | --- | --- | --- |
| **D-1** | `set`/`remove`/`append` expose the skip set only as a list (`skipped`), while `properties rename`/`tags rename` expose only a count (`skipped_count`). No single key answers "how many were skipped" across the mutation family. | R3, R4 | Add `skipped_count` to `set`/`remove`/`append`. The rename commands keep count-only: their skip set is "every scanned file that lacks the property", which on a large vault is the whole vault and carries no information. | no (additive) |
| **D-2** | `lint`'s `results.total` is the violation count (578) while the envelope `total` on the same output is the file count (153). Two different quantities under one name in one document. | R1 | Rename to `results.violations`. | **yes** |
| **D-3** | `links auto`'s `results.total` is the number of matches found — a numerator, and it duplicates `matches.len()`; the denominator on that command is `scanned`. | R1 | Rename to `results.matched`. | **yes** |
| **D-4** | `dry_run` is omitted-when-false on `lint`/`lint --fix` but always present on `set`/`remove`/`append`/`mv`/renames, and entirely absent from `links auto`/`links fix`. `applied` is not its inverse: `links auto --apply` also reports `applied: false` when nothing matched, so a script cannot tell a dry run from an apply that had no work. | R2 | Always emit `dry_run` on `lint`/`lint --fix`; add `dry_run` to `links auto` and `links fix`. | no (jq reads `null` and `false` the same for truthiness; the key only ever gains presence) |
| **D-5** | `summary`'s `schema.files_with_issues` and `lint`'s `files_with_violations` are the same quantity under two names. `output.rs` already carries a compatibility shim reading both. | R4 | Rename `summary`'s to `files_with_violations`. | **yes** |

### Left alone — justified divergence

| ID | Divergence | Why it stays |
| --- | --- | --- |
| **J-1** | `scanned` (mutation family, `links auto`) vs `files_checked` (`lint`) | Same quantity under different verbs, and both are self-describing in their own context. Renaming either churns the text renderer, the GitHub annotation format, hints, the skill template and ~a dozen e2e assertions to buy a synonym. Recorded as a known divergence; a future iteration that introduces a shared count contract should canonicalize then, not now. |
| **J-2** | `links fix` pairs `fixable`→`fixes`, `fuzzy`→`fuzzy_fixes`, `unfixable`→`unfixable_links`, `ambiguous`→`ambiguous_links` | Not drift: the suffix names the record type. `…_fixes` holds fix proposals (`source`, `old_target`, `new_target`, `strategy`, `confidence`); `…_links` holds plain links that have no proposal. A script can tell the two apart from the key alone. |
| **J-3** | `mv`'s `total_files_updated` / `total_links_updated` prefix vs bare counts elsewhere | `mv` acts on exactly one file, so a bare `files_updated` would read as "the file that was moved". The `total_` prefix names the inbound-link fan-out, which is the quantity that actually varies. |
| **J-4** | `config` emits explicit `null`s (`config_path`, `format`, `parse_error`, `raw_contents`, `dir_out_of_bounds_reason`) while `find`'s per-link records omit absent keys | This is R2, working as intended, not a divergence: `config` keys are top-level, `find`'s are per-item. |
| **J-5** | Inside `find`'s link records, `label` is present-but-null while `fragment` and `query` are omitted | `Link` is also the on-disk snapshot-index record, where `skip_serializing_if` is a size optimization on fields most links do not have. `label` is on nearly every markdown link, so omitting it would save nothing and would break `rmp_serde` round-trips without a matching `serde(default)`. Present-but-null is a benign superset of R2. |
| **J-6** | `links auto` counts files (`files_applied`/`files_failed`/`files_skipped`); `links fix` counts links (`applied_fixes`/`failed`/`unapplied`) | Different units, deliberately. `links auto` rewrites whole files in one pass; `links fix` applies per-link fixes. Collapsing the names would make the unit ambiguous. |
| **J-7** | `summary` nests (`files.total`, `links.broken`) where other commands are flat | `summary` is a multi-domain digest; the namespace *is* the information. Flattening would produce `files_total` / `links_total` / `links_broken` collisions. |
| **J-8** | `properties` and `tags` summaries are bare arrays | Already consistent with each other; `properties` adds `type` because a property has one and a tag does not. |
| **J-9** | `LintOutput` (`files_with_issues`, `limited`) vs `ExtLintOutput` (`files_with_violations`, `files_truncated`) | `LintOutput` is no longer reachable from any production path — `lint_files_with_options` is called only from unit tests. Renaming a dead shape is churn; the live drift it caused is D-5. Flagged here so a future cleanup iteration can delete it rather than re-survey it. |

## Not attempted

A general schema/versioning mechanism for the envelope was out of scope by
the iteration's own non-goals. Nothing in this survey argues for one yet:
the divergences found were all name-level, and R1–R4 are enforceable by
review without machinery.
