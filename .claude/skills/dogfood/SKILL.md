---
name: dogfood
description: >
  Run a dogfooding session for hyalo — build from source, exercise the CLI against real
  knowledgebases (own KB, MDN, GitHub Docs, VS Code docs), find bugs, verify recent fixes,
  assess UX, measure performance, and write a structured report. Use this skill whenever
  the user says /dogfood, "dogfood", "run a dogfood session", "test hyalo against real data",
  or wants to exercise hyalo on real knowledgebases to find issues.
user_invocable: true
---

# Dogfood Session

A dogfood session exercises hyalo against real knowledgebases to find bugs, verify fixes,
assess UX, and discover improvement opportunities. Sessions should feel like a real user
working with the tool — not a mechanical checklist. Be curious, follow interesting leads,
try edge cases that occur to you naturally.

## Phase 1: Prepare

1. **Build from source**: `cargo build --release`
2. **Find the last dogfood report** to establish the baseline:
   ```bash
   hyalo find --tag dogfooding --sort date --reverse --limit 1 --format text
   ```
   Read it to learn: which version was tested, which bugs were found, what was verified.
3. **Discover what changed since then**: Use `git log --oneline` from that report's date
   to HEAD. Look for merged iteration branches (`iter-N/...`). For each, read the iteration
   file in the knowledgebase to understand what was implemented:
   ```bash
   hyalo find --tag iteration --property "date>=YYYY-MM-DD" --sort date --format text
   ```
   Then `hyalo read <iteration-file> --format text` for each to understand the features,
   bug fixes, and UX changes. These are your priority test targets.
4. **Check prior dogfood reports** for open bugs to re-verify:
   ```bash
   hyalo find --tag dogfooding --sort date --reverse --limit 5 --format text
   ```
   Read the most recent reports and collect any bugs marked as found (not yet verified fixed).
5. **Pick target knowledgebases**: Use a mix of:
   - **Own KB** (`hyalo-knowledgebase/`, ~250 files) — well-structured, has schemas/views
   - **MDN Web Docs** (`../mdn/files/en-us/`, ~14K files) — large, stress-tests performance
   - **GitHub Docs** (`../docs/content/`, ~3.5K files) — complex nested YAML frontmatter
   - **VS Code Docs** (`../vscode/`, ~1K files) — mid-size, different structure

   For external KBs, check they exist first (`ls ../mdn/files/en-us/ > /dev/null 2>&1`).
   Create a snapshot index for large vaults (500+ files).

## Phase 2: Exercise the tool

This is the creative part. Don't follow a rigid script — think about what a real user would
do with hyalo and try it. That said, make sure every session covers these categories:

### New feature verification
Test the features you discovered in Phase 1 from the iteration files and git log. For each
new feature or changed command, don't just check the happy path — try edge cases, wrong
inputs, combinations with other flags. The iteration files describe what was implemented
and often include acceptance criteria that suggest good test scenarios.

### Bug regression testing
Re-verify bugs from the most recent dogfood report(s). Use the exact commands or scenarios
that originally exposed them. Mark each as STILL FIXED, REGRESSED, or PARTIALLY FIXED.

### Creative exploration
This is where you go beyond verification. Try things like:
- Unusual filter combinations (`--property` + `--tag` + BM25 + `--section`)
- Edge-case inputs (empty strings, special characters, very long queries)
- Commands on files with unusual frontmatter (nested objects, empty values, unicode)
- `--jq` queries to build ad-hoc reports or dashboards
- Views that chain with additional CLI flags
- `mv` on files with many inbound links
- `lint --fix --dry-run` on messy external KBs
- Schema validation on write (`--validate` or `validate_on_write`)
- Performance comparisons (indexed vs non-indexed, small vs large vault)
- `links fix` on KBs with broken links
- Anything that occurs to you as you go — follow your curiosity

### UX assessment
Pay attention to:
- Are error messages helpful? Do they suggest what to do?
- Are hints useful and relevant?
- Is `--format text` output scannable?
- Do help texts match actual behavior?
- Any friction points or confusing behavior?

### Performance spot-checks
Time a few representative commands on each KB used:
```bash
time hyalo find --limit 1                    # baseline parse speed
time hyalo find "some query"                 # BM25 search
time hyalo summary                           # full vault scan
time hyalo find --property status=completed  # structured filter
```

Compare with prior reports if available. Flag regressions > 2x.

## Phase 3: Write the report

Save to `hyalo-knowledgebase/dogfood-results/` with this naming and frontmatter:

```yaml
---
title: "Dogfood vX.Y.Z — <descriptive subtitle>"
type: research
date: YYYY-MM-DD
status: active
tags: [dogfooding, ...]
related:
  - "[[dogfood-results/previous-report]]"
  - "[[iterations/relevant-iteration]]"
---
```

### Report structure

Organize findings into clear sections. Use this general shape, but adapt as needed:

```markdown
# Dogfood vX.Y.Z — <subtitle>

Brief context: binary version, which KBs tested, file counts.

## New Feature Verification
### Feature Name (iter-NNN) — WORKING / BROKEN / PARTIAL
Description of what was tested and results.

## Bug Regression Testing
### BUG-N: Description — STILL FIXED / REGRESSED
Commands tested, results observed.

## Bugs Found
### BUG-N: Description (SEVERITY)
Steps to reproduce, expected vs actual, impact.

## UX Issues
### UX-N: Description (SEVERITY)
What's confusing or could be better.

## What Worked Well
Highlight things that are genuinely good — performance wins, helpful errors, etc.

## Performance
Table of timings if measured.
```

Severity levels: HIGH (data loss, wrong results), MEDIUM (confusing behavior, workaround
exists), LOW (cosmetic, minor friction).

### After writing the report

Use `hyalo set` to ensure the frontmatter is correct. Add `related` links to relevant
iterations or prior dogfood reports.

## Tips for good dogfood sessions

- **Follow the hints.** Hyalo outputs drill-down suggestions — use them like a real user would.
- **Don't skip external KBs.** Many bugs only surface at scale or with unusual frontmatter.
  The own KB is too well-structured to catch everything.
- **Note what's NOT there.** Missing features, things you wished you could do — these are
  just as valuable as bugs.
- **Be specific.** Include exact commands, exact error messages, exact output. Vague reports
  like "search felt slow" aren't actionable.
- **Test across KBs.** A command that works on the own KB might fail on MDN due to scale or
  on GitHub Docs due to nested YAML.
- **Check data quality.** Use `lint`, `find --broken-links`, `find --orphan` to surface
  issues in the KB itself — this is valuable dogfooding too.
