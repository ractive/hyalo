---
title: Iteration 127 — Markdown linter follow-up (bugs + UX from dogfood v0.14.0)
type: iteration
date: 2026-05-04
status: completed
tags:
  - iteration
  - lint
  - bug-fix
  - ux
branch: iter-127/lint-followup
---

## Goal

Address all findings from [[dogfood-results/dogfood-v0140-iter126-markdown-linter]] against the iter-126 markdown linter (4 bugs + 6 UX issues after consolidation; BUG-5 and UX-5 are dropped because they only existed for the deleted HYALO002), plus one skill-template documentation gap.

This iteration also drops the HYALO002 (title↔H1 agreement) rule entirely, on the grounds that it codifies a minority convention (filename ≠ H1 is the dominant Obsidian/Logseq/Foam style) and a project that wants the check can already get it via `[schema.types.*].properties.title.pattern`. The remaining cross-cutting rule (formerly HYALO003) is renamed to **HYALO002** so the catalog stays contiguous.

iter-126 has not been released, so this is a fix-forward iteration: no migration shims, no deprecation warnings, no carry-over handling. The pre-release behaviour just becomes the released behaviour.

Numbering note: BUG and UX numbers below mirror the dogfood report's numbering for traceability — BUG-5 and UX-5 are intentionally absent.

## Context

Iter-126 shipped the markdown linter (mdbook-lint embed + HYALO001/002/003 + `lint-rules` CLI). The dogfood pass against own KB, personal Obsidian vault, VS Code docs, GitHub Docs, and MDN found two crashes/spec-divergences in core paths and several ergonomic gaps that hurt agent discoverability.

Throughout this iteration, **HYALO002** refers to the *renamed* rule (formerly HYALO003 — `status: completed` with open tasks). The original HYALO002 (title↔H1 agreement) is removed.

## High

### BUG-1: `lint-rules set --severity` panics on scalar→table promotion

**Bug:** When `[lint.rules]` already contains a scalar entry like `MD013 = true`, running `hyalo lint-rules set MD013 --severity error` panics with `index not found` at `crates/hyalo-cli/src/commands/lint_rules.rs:198`. The iter-126 plan explicitly documents this promotion path (*"`set` writes scalar form … when only `--enabled` is given; promotes to table form … when `--severity` is also given"*).

**Fix:** At the promotion site, detect the existing scalar leaf, remove it, then insert the new table — instead of indexing into a child that doesn't exist on a scalar.

- [x] Reproduce with the exact sequence from the dogfood report (`set --enabled true` → `set --severity error`)
- [x] Implement the promotion: when the existing entry is a scalar bool, replace with a table containing both `enabled` (preserved from the scalar) and `severity` (from the new flag)
- [x] Cover the inverse: scalar `false` → table with `enabled = false` + `severity = X`
- [x] Unit test for both directions of the promotion

### BUG-2: `--fix` JSON envelope diverges from spec

**Bug:** Iter-126 plan documents this fix-mode envelope:

```json
{"results": {"files": [
   {"file": "foo.md",
    "fixed_groups":     [{"rule": "MD009", "count": 4}],
    "remaining_groups": [{"rule": "MD013", "count": 8, ...}],
    "conflicts":        [{"rule": "MD047", "reason": "range overlap with MD009"}]}],
 "total_fixed": 12, "total_remaining": 8, "total_conflicts": 1}}
```

The actual implementation reuses the read-only `rule_groups` shape with a `dry_run: true` flag. None of `fixed_groups`/`remaining_groups`/`conflicts`/`total_fixed`/`total_remaining`/`total_conflicts` exist. The fix-mode payload is structurally indistinguishable from a read-only run, so the count of fixed vs remaining isn't recoverable.

**Fix:** Split the violation list at the point we apply fixes. Collect successfully-applied fixes into `fixed_groups`, leftover violations (unfixable + deferred-by-conflict) into `remaining_groups`, and overlap-detected fixes into `conflicts`.

- [x] Define the per-file `FixOutcome { fixed_groups, remaining_groups, conflicts }` struct in `hyalo-mdlint`
- [x] Update the body-fix orchestration to emit a `FixOutcome` per file instead of a `Vec<RuleGroup>`
- [x] Aggregate top-level `total_fixed`, `total_remaining`, `total_conflicts` across all files
- [x] Update the JSON serializer for `--fix` to emit the spec'd shape
- [x] Keep the read-only `--fix --dry-run` path returning the same `fixed_groups`/`remaining_groups` shape (just no actual writes) — `dry_run: true` continues to mark it
- [x] Update `hyalo lint --help` if any flag descriptions referenced the old shape

## Rule catalog consolidation

### Remove old HYALO002 (title↔H1 agreement)

**Rationale:** The rule codifies a minority convention. Obsidian / Logseq / Foam / Dendron all treat filename as identity and H1 as first content — disagreement is the norm, not a bug. Projects that genuinely want title↔H1 agreement can already enforce it via `[schema.types.*].properties.title.pattern` matching the body's first H1 capture, or by convention review.

The rule earns no rent on the dominant ecosystem and the dogfood pass found exactly zero true positives on the personal Obsidian vault (9/9 firings were legitimate-by-convention). It also costs the implementation a `mode` config (`match | either | off`), invalid-mode validation, default-on/off curation debate, and skill-template caveats.

**Scope:** delete the rule, its tests, its `mode` parsing, and the curation-table entries. Drop UX-5 (default-on debate) and BUG-5 (mode validation) from the dogfood report — both moot.

- [x] Delete `hyalo002.rs` (the title↔H1 rule) from `crates/hyalo-mdlint`
- [x] Remove the rule from `HyaloRuleProvider` registration
- [x] Remove `mode` config parsing for `[lint.rules.HYALO002]` from `.hyalo.toml` schema
- [x] Remove HYALO002 from severity-override and default-enabled tables
- [x] Remove unit tests for the title↔H1 rule
- [x] Remove HYALO002 references from README, both skill templates, project CLAUDE.md, and `rule-knowledgebase.md` template

### Rename HYALO003 → HYALO002

**Rationale:** with the old HYALO002 gone, the only remaining cross-cutting rule beyond HYALO001 is the status-vs-tasks check. Renaming `HYALO003` → `HYALO002` keeps the catalog contiguous and matches user expectation that rule IDs are dense.

- [x] Rename rule struct `Hyalo003` → `Hyalo002` and file `hyalo003.rs` → `hyalo002.rs`
- [x] Update rule ID constant `"HYALO003"` → `"HYALO002"` everywhere in `hyalo-mdlint`
- [x] Update severity-override and default-enabled tables to reference `HYALO002`
- [x] Update the rule's diagnostic message format (still includes `HYALO002` in the prefix)
- [x] Update fixture files that reference `HYALO003` in expected output
- [x] Update README, both skill templates, project CLAUDE.md, and decision-log entries to use `HYALO002` (the new ID)

## Medium

### BUG-3: `--fix` text output is indistinguishable from read-only lint

**Bug:** Once BUG-2 is fixed, the data is available — but text mode currently prints the same violation list as a read-only lint, so users have no signal whether anything was written.

**Fix:** Render fix outcomes in text mode with a clear summary. Proposed shape:

```text
test.md:
  fixed   HYALO001  line 4   bare checkbox `[]` (replaced with `- [ ]`)
  fixed   MD009     line 8   trailing spaces (removed 3)
  fixed   MD034     line 9   bare URL (wrapped in angle brackets)
  remain  HYALO002  line 21  status is `completed` but 1 task remains unchecked
1 file checked: 3 fixed, 1 remaining, 0 conflicts.
```

- [x] Render `fixed` / `remain` / `conflict` prefix per violation in text mode (all three must be visually distinct)
- [x] Bottom-line summary uses the new totals from BUG-2 (`fixed N · remaining M · conflicts K`)
- [x] Read-only lint output unchanged

### UX-1: Per-rule hint chains for HYALO001/HYALO002 are missing

**Bug:** Iter-126 promised per-rule hint chains; none of them fire today. With the catalog consolidation, the surviving chains are:
- HYALO001 → `→ hyalo lint --rule HYALO001 --fix`
- HYALO002 (renamed from HYALO003) → `→ hyalo find --task todo --file <FILE>  # see open tasks`

**Fix:** Add a per-rule hint emitter to the lint hint pipeline. Conditional on the rule firing in the current run, capped at `MAX_HINTS = 5`.

- [x] Add a `per_rule_hints(rule_id, file)` helper in the lint hint module returning the chain entry (easily extensible for future rules)
- [x] Wire into the lint hint pipeline so each fired rule contributes one chain entry, deduplicated when the same rule fires across many files (use the worst-offender file as the example)
- [x] HYALO002 chain uses `hyalo find --task todo --file <FILE>`
- [x] Verify chain still respects `--no-hints` / `--jq` suppression
- [x] E2E test: lint a fixture firing each rule, assert the chain is in `hints[]`

### UX-2: No `lint-rules` hint when one rule dominates the noise

**Bug:** Lint of VS Code docs surfaces 730/1122 violations (65%) from MD010, but the hint chain doesn't mention `lint-rules`. The strongest signal that a rule's defaults don't fit the KB is that one rule dwarfs the others — that's the moment to nudge toward tuning.

**Fix:** When a single rule accounts for ≥50% of total violations *and* has at least 50 absolute violations, emit:

```text
→ hyalo lint-rules show <RULE>          # consider tuning if too noisy on this KB
```

Threshold rationale: 50% catches "one rule dominates"; 50 absolute prevents the hint firing on tiny vaults where one rule in a 5-file vault is 100% but irrelevant. Both numbers stay constants in source.

- [x] Compute per-rule totals and the dominance ratio in the lint summary phase
- [x] Emit the `lint-rules show` hint when both thresholds are met
- [x] Subject to the `MAX_HINTS = 5` cap (rank below `--fix` hints, above the generic `--limit 0`)
- [x] E2E test: synthetic vault with 60+ MD010 violations and a sprinkling of others, assert the hint fires

### UX-3: `lint-rules list --format text` is unscannable

**Bug:** Current text mode dumps 9 YAML-style key/value lines per rule × ~58 rules (one fewer after removal). There is no way to scan for what's enabled vs disabled, what's autofixable, or what HYALO/MD groupings exist.

**Fix:** Render as a column table with a one-line summary footer. Columns: ID, NAME, SEVERITY, ENABLED, AUTOFIX, DESCRIPTION (truncated to terminal width).

```text
ID         NAME                  SEVERITY   ENABLED   AUTOFIX   DESCRIPTION
MD001      heading-increment     warn       on        yes       Heading levels should only increment by one level …
MD009      no-trailing-spaces    warn       on        yes       Trailing spaces detected
…
HYALO001   bare-checkbox         error      on        yes       Bare `[]` should be `- [ ]`
HYALO002   completed-tasks       error      on        no        `status: completed` requires all task checkboxes ticked

58 rules, 16 enabled, 42 disabled. 35 autofixable.
  -> hyalo lint-rules show <ID>                  # full details for a rule
  -> hyalo lint-rules set <ID> --enabled false   # disable
```

- [x] Implement column layout sized to terminal width (use existing terminal-width helper if present, otherwise wrap at 120 cols)
- [x] Group HYALO rules visually before / after MD rules (sort by source then ID — keeps HYALO00X together)
- [x] Footer summary line with totals
- [x] JSON output unchanged
- [x] E2E test: `--format text` output contains a header row, footer summary, and one row per rule

### UX-4: `effective_enabled: true` lies for HYALO002 without a schema

**Bug:** HYALO002 (renamed) fires only when `[schema.types.*].properties.status` is declared as an enum with a `completed` value — by design, to avoid false positives in vaults that use "completed" with different semantics. But `lint-rules show HYALO002` reports `effective_enabled: true` regardless. A user debugging "why doesn't HYALO002 catch my completed-with-open-tasks file?" has no signal.

**Fix:** Add an `activation` field to `lint-rules show` output for rules with non-trivial activation predicates. For HYALO002, include the precondition and whether it's currently met.

```text
$ hyalo lint-rules show HYALO002
id: HYALO002
…
activation:
  predicate: schema declares `status` as enum containing "completed"
  satisfied: false   # ← reflects the current `.hyalo.toml`
```

- [x] Add an `activation: Option<{predicate, satisfied}>` field to the show command's output
- [x] HYALO002 populates it; HYALO001 leaves it as `null`
- [x] Both JSON and text mode render it
- [x] E2E test: `lint-rules show HYALO002` on a vault with no schema returns `satisfied: false`; with the schema, returns `satisfied: true`

## Low

### BUG-4: HYALO002 grammar bug

**Bug:** `1 task remain unchecked` (singular needs "remains").

**Fix:** Pluralise the verb correctly: `remains` for 1, `remain` for ≠ 1.

- [x] Fix the message format string in HYALO002
- [x] Unit test asserts both singular and plural forms

### UX-6: `lint-rules set` output is unfriendly

**Bug:** Output is structured but not friendly — no "before → after", no path to the modified `.hyalo.toml`, no diff-like summary.

**Fix:** Echo the change in a human-scannable form (in text mode) while keeping the structured JSON unchanged.

```text
$ hyalo lint-rules set HYALO002 --enabled false
HYALO002: enabled on → off (warn → warn)
  wrote .hyalo.toml
```

- [x] Render text mode with before/after diff
- [x] Echo the path of the modified config file
- [x] JSON output unchanged
- [x] Same shape for `set --severity`, `set --enabled false`, and `remove`

### UX-7: `--rule-prefix HYALO --fix --dry-run` doesn't suggest applying

**Bug:** When the user is already in fix-dry-run mode, the hint chain still suggests `--fix --dry-run` (no-op since they're already there). The obvious next step — apply — is missing.

**Fix:** The hint engine should know which mode the user is in and only suggest the *next* step in the workflow.

- [x] Suppress the `--fix --dry-run` hint when `--dry-run` is already on
- [x] Suppress the `--fix` hint when it's already on (no actual write happened — `--dry-run` was on)
- [x] When `--fix --dry-run` was on AND there were autofixable violations, emit the apply hint preserving the user's filter flags (e.g. preserve `--rule-prefix HYALO`)

## Skill template updates

One documentation gap surfaced by the dogfood pass — neither hyalo nor hyalo-tidy mentions HYALO002's schema precondition.

- [x] Add a one-line note in `crates/hyalo-cli/templates/skill-hyalo.md` (Schema & Lint section): "HYALO002 fires only when `[schema.types.*].properties.status` is declared as an enum with `completed`"
- [x] Same note in `crates/hyalo-cli/templates/skill-hyalo-tidy.md` near the Phase 3 lint step
- [x] Same one-liner in `README.md` lint section
- [x] Remove all references to the old HYALO002 (title↔H1) from these docs and from `crates/hyalo-cli/templates/rule-knowledgebase.md`

## Verification

- [x] Re-run the dogfood scenarios from [[dogfood-results/dogfood-v0140-iter126-markdown-linter]] (BUG-1 panic repro, `--fix` envelope shape, top-level hint output) against the merged branch to verify the listed issues are gone — no need for a full new dogfood report, just a one-liner per fix in the iteration's completion note

## Quality gates (per CLAUDE.md, in order, must pass)

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace -q`

## Acceptance criteria

- [x] BUG-1 reproducer no longer panics; promotion writes the expected table form
- [x] `hyalo lint --fix --format json` returns the spec'd `fixed_groups` / `remaining_groups` / `conflicts` / `total_*` envelope
- [x] `hyalo lint --fix --format text` shows a `fixed N · remaining M · conflicts K` summary
- [x] Old HYALO002 (title↔H1) is gone — `lint-rules show HYALO002` returns the renamed rule (status↔tasks), not the deleted one
- [x] HYALO001 and HYALO002 each emit per-rule hint chains
- [x] Vault with one rule dominating produces a `lint-rules show` hint suggesting tuning
- [x] `hyalo lint-rules list --format text` renders as a column table with footer summary
- [x] `hyalo lint-rules show HYALO002` reports `activation.satisfied` based on the current schema
- [x] HYALO002 message uses correct singular grammar
- [x] `lint-rules set` text output shows before/after and the modified config path
- [x] Dry-run apply hint suggests the apply step (with filter flags preserved) only when not already in apply mode
- [x] Skill templates and README mention HYALO002's schema precondition; old HYALO002 is removed everywhere
- [x] All quality gates pass

## Out of scope (deferred to follow-up iterations)

- Per-rule arg pass-through to mdbook-lint-core (still blocked on the toml 0.5 vs 1.x version translation layer — same as iter-126)
- New HYALO native rules (HYALO003+) — see follow-up brainstorm; not this iteration
- Rule-catalog change-detection in upstream `mdbook-lint-rulesets` minor versions (iter-126 risk note)
- LSP integration
