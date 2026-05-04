---
title: "Dogfood v0.14.0 — iter-126 markdown linter + agent ergonomics"
type: research
date: 2026-05-04
status: active
tags:
  - dogfooding
  - lint
  - ux
  - llm
  - skills
related:
  - "[[dogfood-results/dogfood-v0120-post-iter122]]"
  - "[[iterations/iteration-126-markdown-linter]]"
---

# Dogfood v0.14.0 — iter-126 markdown linter + agent ergonomics

Binary: `hyalo 0.14.0` (post-iter-126 merge, commit `e9846d9`).
KBs tested: own KB (257 files), personal Obsidian vault (227 files), VS Code docs (780 files), GitHub Docs (7,354 files), MDN Web Docs (14,264 files).

Focus: iter-126 (mdbook-lint embed + HYALO001/002/003 + `lint-rules` CLI) and how an AI agent discovers / reaches for the new features. Cross-checked the `hyalo` and `hyalo-tidy` skill templates for consistency.

## What works

- **Top-level discoverability is good.** `hyalo --help` lists `lint` with the new tagline ("frontmatter (schema) and markdown body (mdbook-lint + HYALO native rules)") and `lint-rules` as its own command.
- **Body pass fires correctly across all 5 KBs.** mdbook-lint catalog active, HYALO001/002/003 detect the right cases.
- **HYALO001 autofix is solid.** Bare `[]` → `- [ ]` works on isolated cases, idempotent on rerun. MD009 (trailing whitespace) and MD034 (bare URL → angle brackets) also autofix correctly.
- **`--rule`, `--rule-prefix`, `--max-per-rule`, `--detailed`, `--fix-rule` all work as specified.**
- **Edge cases handled**: empty files, files without frontmatter, broken frontmatter (clear `FRONTMATTER` error), 5000-char single-line files (MD013 fires when enabled).
- **Validation is courteous**: `hyalo lint-rules show MD9999` and `lint-rules set MD9999 --enabled false` both return `Error: no such rule: MD9999\n  hint: run \`hyalo lint-rules list\` to see available rules`.
- **Performance scales**: see [Performance](#performance).

## Bugs Found

### BUG-1: `lint-rules set --severity` panics on scalar→table promotion (HIGH)

When a rule already has a scalar entry like `MD013 = true` in `[lint.rules]`, attempting to add a severity panics:

```bash
$ hyalo lint-rules set MD013 --enabled true
$ cat .hyalo.toml
[lint.rules]
MD013 = true

$ hyalo lint-rules set MD013 --severity error
thread 'main' panicked at crates/hyalo-cli/src/commands/lint_rules.rs:198:23:
index not found
```

The iteration plan explicitly calls out this exact promotion path as a feature: *"`set` writes scalar form … when only `--enabled` is given; promotes to table form … when `--severity` is also given."*

Reproducible 100%. Workaround: hand-edit `.hyalo.toml` to convert the scalar to a table first. A panic on a documented mutation path is the most severe finding of the session.

### BUG-2: `--fix` JSON envelope diverges from spec (HIGH)

iter-126 documented this fix-mode envelope:

```json
{"results": {"files": [
   {"file": "foo.md",
    "fixed_groups":     [{"rule": "MD009", "count": 4}],
    "remaining_groups": [{"rule": "MD013", "count": 8, ...}],
    "conflicts":        [{"rule": "MD047", "reason": "range overlap with MD009"}]}],
 "total_fixed": 12, "total_remaining": 8, "total_conflicts": 1}}
```

The actual implementation reuses the read-only `rule_groups` shape with a `dry_run: true` flag — none of `fixed_groups`, `remaining_groups`, `conflicts`, `total_fixed`, `total_remaining`, `total_conflicts` exist. Confirmed via `--rule-prefix HYALO --fix --dry-run --format json` and `--fix --format json`.

Impact: agents scripting against the documented shape will silently get incorrect data — the `rule_groups[].count` includes both fixed *and* remaining violations indistinguishably.

### BUG-3: `--fix` text output is indistinguishable from read-only lint (MEDIUM)

`hyalo lint --fix` reports the same violation list as a non-fix run, so the user has no signal whether anything was actually written:

```console
$ hyalo lint --fix
test.md:
  error  HYALO001  line 4  bare checkbox `[]` on line 4 — should be `- [ ]`
  warn   MD009  line 8  Trailing spaces detected (found 3 trailing spaces)
  warn   MD034  line 9  Bare URL used: ...
1 file checked, 1 with issues (2 errors, 3 warnings)

$ # File was actually modified — re-running shows no issues, but the previous output
$ # gave no hint that 4/5 violations were fixed.
```

Expected: a "fixed N of M; M-N remain" summary, or a separate `Fixed:` section. Pairs with BUG-2 — once the JSON shape is fixed, text mode can render the same info.

### BUG-4: HYALO003 message uses incorrect singular grammar (LOW)

```text
error  HYALO003  line 21  status is `completed` but 1 task remain unchecked (first at line 21)
```

Should be "1 task remains" or "1 tasks remain" (the latter is what's already used for >1). Trivial copy fix in the rule body. Surfaced 8× in own-KB lint of `iterations/done/` files.

### BUG-5: Invalid `mode` value silently falls back to default (LOW)

```toml
[lint.rules.HYALO002]
mode = "bogus_mode"   # not in [match | either | off]
```

Linter runs as if mode were `either` (default). No warning, no error. Typos in config are easy to make and silently ignored, so users have no way to discover their override isn't taking effect.

Expected: warn at startup when an unknown `mode` value is encountered (consistent with how other config validation works in iter-125).

## UX Issues

### UX-1: Per-rule hint chains promised in iter-126 are missing (MEDIUM)

The iteration plan specified per-rule hint chains:
- HYALO001 → `→ hyalo lint --rule HYALO001 --fix`
- HYALO002 → `→ hyalo read --file <FILE> --lines 1:5  # compare title vs H1`
- HYALO003 → `→ hyalo find --task todo --file <FILE>  # see open tasks`

None of these fire. `hyalo lint --rule HYALO00X` returns only generic hints (`--limit 0`, `types list`). For HYALO001 the generic `--fix --dry-run` / `--fix` hints partially cover it; for HYALO002 and HYALO003 there is no chain at all.

The hint chains were one of the ergonomics headlines of the iteration ("output is shaped for AI agents") — losing them removes a meaningful drill-down for any AI consumer.

### UX-2: No `lint-rules` hint when one rule dominates the noise (MEDIUM)

VS Code docs lint:
- Total 1,122 violations, 730 from MD010 alone.
- Hints emitted: `--fix --dry-run`, `--fix`, `types list`, `--limit 0`. None point at `lint-rules`.

If a single rule produces >50% of the noise on a given KB, that's the strongest possible signal that the rule's defaults don't fit and the user should consider tuning it. The iteration spec included this kind of nudge in the design intent ("output is shaped for AI agents — per-rule caps, summary mode, hint chains") but the chain to `lint-rules show <RULE>` and `lint-rules set <RULE> --enabled false` doesn't appear.

### UX-3: `lint-rules list --format text` is unscannable (MEDIUM)

The output is a flat 9-key YAML-style dump per rule, repeated for ~59 rules:

```text
autofixable: true
default_enabled: true
default_severity: warn
description: Heading levels should only increment by one level at a time
effective_enabled: true
effective_severity: warn
has_override: false
id: MD001
name: heading-increment
source: mdbook-lint-rulesets
autofixable: true
default_enabled: false
...
```

Compare with `hyalo types list --format text` which is one line per type with key info. For a catalog with ~59 entries you cannot scan to find what's enabled vs disabled. A column table would fix this:

```text
ID         NAME                  SEVERITY   ENABLED   AUTOFIX   DESCRIPTION
MD001      heading-increment     warn       on        yes       Heading levels should only increment by one level at a time
MD002      first-heading-h1      warn       off       yes       First heading should be a top-level heading
...
HYALO001   bare-checkbox         error      on        yes       Bare `[]` should be `- [ ]`
HYALO002   title-h1-agreement    warn       on        no        Frontmatter `title` should agree with first H1
HYALO003   completed-tasks       error      on        no        `status: completed` requires all task checkboxes ticked

59 rules, 17 enabled, 42 disabled. 35 autofixable.
```

### UX-4: `effective_enabled: true` for HYALO003 is misleading without a schema (MEDIUM)

HYALO003 fires only when `.hyalo.toml` declares `status` as an `enum` with a `completed` value — by design, to avoid false positives in vaults that use "completed" with different semantics.

But `hyalo lint-rules show HYALO003` reports `effective_enabled: true` regardless. On the personal Obsidian vault (no schema), the rule is silently a no-op while claiming to be active. A user trying to debug "why doesn't HYALO003 catch my completed-with-open-tasks file?" has no signal.

Two possible fixes:
- (a) `effective_enabled` returns `true (no-op: requires schema with status enum)` when the schema isn't configured, or
- (b) `lint-rules show HYALO003` includes an `activation` field describing the precondition.

### UX-5: HYALO002 default-on is wrong for typical Obsidian usage (LOW)

Obsidian convention is *filename = title* (frontmatter `title` echoes the filename) and the *H1 is the first content line* (often a date stamp like `21.03.25`, a person's name, or a meeting topic). Linting the personal vault produced 9 HYALO002 warnings, all of them legitimate-by-convention. A user encountering a brand-new Obsidian vault would need to find and disable HYALO002 before the rule's noise drowns out anything useful.

The iteration plan's "Risks" section already calls this out as an open question (*"HYALO002's `mode: either` default may surprise KBs that intentionally keep title and H1 different"*) and the right fix is probably to ship HYALO002 default-off and document it as something to opt into for projects that enforce title↔H1 agreement (like our own iterations).

### UX-6: `lint-rules set` output is unfriendly (LOW)

```text
$ hyalo lint-rules set HYALO002 --enabled false
action: set
dry_run: false
enabled: false
rule_id: HYALO002
severity: null
```

No "before → after", no confirmation that the file was modified, no path to the modified `.hyalo.toml`. Compare with `hyalo set` which echoes the changed file paths and a diff-like summary.

### UX-7: `--rule-prefix HYALO --fix --dry-run` doesn't suggest applying (LOW)

Running `lint --rule-prefix HYALO --fix --dry-run` (clearly an interactive preview workflow) returns hints about `--limit 0` and `types list` — not the obvious next step `lint --rule-prefix HYALO --fix` (apply). The hint engine isn't aware of which mode the user is already in.

## Skill coordination findings

The `hyalo` skill (`crates/hyalo-cli/templates/skill-hyalo.md`) and `hyalo-tidy` skill (`skill-hyalo-tidy.md`) were updated in commit `e9846d9` to teach the new lint capabilities. Verified both refer to:

- `hyalo lint` covering both passes
- HYALO001/002/003 by name
- `hyalo lint-rules` as the management surface
- `--rule-prefix HYALO`, `--fix`, etc.

What's still missing in both skills:

- **HYALO003's schema precondition is not mentioned anywhere.** An agent following the tidy skill on a fresh KB would see HYALO003 silently never fire and would have no clue why. A one-liner ("HYALO003 only fires when `[schema.types.*].properties.status` is declared as an enum") in both skills would close this gap.
- **HYALO002's Obsidian-noisiness warning isn't mentioned.** When the tidy skill suggests running `hyalo lint`, an agent on an Obsidian-style vault could blindly apply HYALO002's "fix" (rename the H1 to match the title) and destroy the user's content. The skill should call out that HYALO002 is opinionated and the right response on a clean Obsidian vault is to disable it.
- The tidy skill's existing `completed-with-todos` view + new HYALO003 reference is good — but with HYALO003 silently dormant on schema-less vaults, the tidy skill should fall back to the view explicitly. Currently it implies HYALO003 covers the case.

The two skills don't conflict — they cover orthogonal surfaces (general CLI vs orchestrated tidy pass). No coordination bugs.

## Performance

| Vault | Files | Wall | CPU | Notes |
|---|---|---|---|---|
| Own KB | 257 | 0.13 s | 1.0 s | full body lint, default rules |
| Personal vault | 227 | 0.10 s | 0.7 s | Obsidian style, mostly clean |
| VS Code docs | 780 | 0.49 s | 4.06 s | rayon scaling visible |
| GitHub Docs | 7,354 | 0.86 s | 7.03 s | 882 files with violations |
| MDN Web Docs | 14,264 | 2.48 s | 20.40 s | mostly HTML-in-md, very clean |

No regressions vs prior baselines (BM25 indexed 0.66s on MDN held). Body pass adds ~10–15% over frontmatter-only — well within the budget.

## Recommendations

Priority order for a follow-up iteration:

1. **Fix BUG-1 panic** (scalar→table promotion). Trivial scope, blocks a documented user path.
2. **Fix BUG-2 / BUG-3 fix-mode envelope.** Implement `fixed_groups`/`remaining_groups`/`conflicts`/`total_fixed`/`total_remaining`/`total_conflicts` per spec; render them in text mode as a "Fixed N · Remaining M · Conflicts K" summary.
3. **Restore per-rule hint chains (UX-1).** This was a headline feature of the iteration. Adding three rule-specific hint emitters to the existing hint pipeline.
4. **Add a "rule-dominates" hint (UX-2).** When a single rule accounts for >50% of total violations, append `→ hyalo lint-rules show <RULE>  # consider tuning if too noisy`.
5. **Reflow `lint-rules list --format text` into a column table (UX-3).**
6. **Reconsider HYALO002 default-on (UX-5).** Likely flip to default-off and document as an opt-in rule for projects with strong title↔H1 conventions.
7. **Surface HYALO003's schema precondition** in `lint-rules show` output (UX-4) and in both skill templates.
8. **Validate `mode` values at config load time (BUG-5).** Warn on unknown values.
9. **Fix grammar bug (BUG-4).**

## What was NOT regressed

- iter-123/124 auto-link: spot-checked `hyalo links auto --first-only --exclude-target-glob 'templates/*'` on own KB → 65 candidates surfaced cleanly, no panics, no false positives in the structured output.
- iter-125 review-fix items: not retested in detail; no obvious surface-level problems.
- iter-122 security hardening: no surface change observed across all 5 KBs (no new panics, no path-traversal regressions, sanitization still working).
- Snapshot index, `mv`, `find`, `set`, `task` — all stable; verified via the 50-file lint output capture and ad-hoc `find` queries while preparing the report.
