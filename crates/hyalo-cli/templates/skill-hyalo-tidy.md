---
name: hyalo-tidy
description: >
  Perform a reflective consolidation pass over a hyalo-managed knowledgebase directory —
  detecting structural issues, fixing broken links, flagging stale content, normalizing
  metadata, and reporting what changed. Use this skill when the user says /hyalo-tidy,
  "consolidate the knowledgebase", "clean up the KB", "run KB hygiene", "tidy", or
  "what needs attention in the knowledgebase". Also use when the user asks about
  knowledgebase health, broken links, orphan files, stale iterations, or metadata
  inconsistencies.
context: fork
disable-model-invocation: true
---

# Hyalo Tidy — Knowledgebase Consolidation

You are performing a tidy — a reflective pass over a knowledgebase. Your job is to
detect issues, fix what you can, and report what needs human attention. Think of this
as a librarian doing a periodic shelf-read: checking that everything is filed correctly,
cross-references work, and nothing is gathering dust in the wrong place.

This process has 5 phases. Take your time — a thorough tidy is worth more than a fast
one. A few minutes is fine.

## Before you start

Locate the hyalo binary:
```bash
which hyalo 2>/dev/null || echo "target/release/hyalo"
```

Confirm `.hyalo.toml` exists in the project root to determine the KB directory. If it
doesn't exist, ask the user which directory to consolidate.

## Phase 1 — Orient and snapshot

Get the lay of the land and create a snapshot index for fast repeated queries.

```bash
# 1. High-level overview (baseline for the final health dashboard)
hyalo summary --format text

# 2. Create snapshot index (one scan, reused by all subsequent queries)
hyalo create-index

# 3. Save recurring diagnostic queries as views for reuse
hyalo views set stale-in-progress --property status=in-progress --fields tasks
hyalo views set missing-status --property '!status'
hyalo views set missing-type --property '!type'
hyalo views set orphans --fields backlinks
hyalo views set completed-with-todos --property status=completed --task todo --fields tasks
```

The snapshot index captures every file's metadata in a binary file (`.hyalo-index`).
All read-only queries in Phase 2 and Phase 3 should use `--index .hyalo-index` to
avoid repeated disk scans. For complex reshaping, combine hyalo filtering with `--jq`.

Also grab the tag vocabulary for inconsistency detection:
```bash
hyalo tags summary --format text --index .hyalo-index
```

## Phase 2 — Gather recent signal

Before looking for issues, understand what happened recently. This context is what
makes the tidy valuable — it tells you what *should* have changed in the KB, so you
can spot what didn't.

### Git history

```bash
# What branches were merged recently? What iteration branches shipped?
git log --oneline --merges --since="4 weeks ago"

# What areas of code changed? (helps identify which features shipped)
git log --oneline --since="4 weeks ago" -- "*.rs" | head -30
```

Extract non-completed iterations and their branches from the index:
```bash
hyalo find --property type=iteration --index .hyalo-index --jq '.results | map(select(.properties.status != "completed" and .properties.status != "superseded" and .properties.status != "wont-do")) | map({file, branch: .properties.branch, status: .properties.status})'
```

For each non-completed iteration that has a branch, check if that branch was merged:
```bash
git log --oneline --merges --all | grep "<branch-name>"
```

### Claude's auto-memory

Check what was recently worked on from Claude's perspective:
```bash
MEMORY_FILE=$(find ~/.claude/projects/ -path "*/memory/MEMORY.md" -print -quit 2>/dev/null)
if [ -n "$MEMORY_FILE" ]; then
  MEMORY_DIR=$(dirname "$MEMORY_FILE")
fi
```

If found, query it with hyalo (memory files are outside the vault, so no --index here):
```bash
hyalo --dir "$MEMORY_DIR" find --property type=project --format text
hyalo --dir "$MEMORY_DIR" find --property type=feedback --format text
```

Look for:
- Project memories mentioning iterations/features that shipped — cross-reference with KB
- Stale project memories that reference outdated plans (note for the report, but
  don't modify memory files — that's Claude's own territory)

### Recent KB changes

```bash
git log --oneline --since="4 weeks ago" -- "hyalo-knowledgebase/" | head -20
git log --diff-filter=A --name-only --since="4 weeks ago" -- "hyalo-knowledgebase/"
```

## Phase 3 — Detect structural issues

All queries below use `--index .hyalo-index` — no additional disk scans needed.

### Broken links
```bash
# Dry-run shows broken links with proposed fixes and confidence scores
hyalo links fix --index .hyalo-index --format text
```
This categorizes links as **fixable** (fuzzy match found) vs **unfixable** (no match).
Note the counts for the health dashboard. Actual fixes happen in Phase 4.

### Orphan files
```bash
hyalo find --fields backlinks --index .hyalo-index --jq '.results | map(select(.backlinks | length == 0)) | map(.file)'
```
Not all orphans are problems. Expect these to be legitimately orphaned:
- Top-level files (SEED.md, project-pitch.md, decision-log.md)
- Research documents (standalone reports)
- Older completed items in `done/` directories

Focus on **actionable orphans**: active/planned items that should be cross-referenced.

### Stale statuses
```bash
# In-progress items — should any be completed?
hyalo find --view stale-in-progress --index .hyalo-index --jq '.results | map({file, date: .properties.date, branch: .properties.branch})'

# Planned items where all tasks are done
hyalo find --property status=planned --index .hyalo-index --jq '.results | map(select((.tasks | length > 0) and ([.tasks[] | select(.status != "x")] | length) == 0)) | map(.file)'

# In-progress items sorted by date (oldest first — possibly stale)
hyalo find --view stale-in-progress --index .hyalo-index --jq '.results | map(select(.properties.date != null)) | sort_by(.properties.date) | map({file, date: .properties.date})'
```
Cross-reference with git merges from Phase 2. If the branch was merged, update status.

### Stale backlog items
```bash
hyalo find --property status=planned --property type=backlog --index .hyalo-index --jq '.results | map({file, title: .properties.title})'
```
Compare each planned backlog item against merged iterations and recent git history.
If the feature clearly shipped (in a different iteration or under a different name),
flag it.

### Missing metadata
```bash
hyalo find --view missing-status --index .hyalo-index --jq '.results | map(.file)'
hyalo find --view missing-type --index .hyalo-index --jq '.results | map(.file)'
```

### Tag inconsistencies
Review the `hyalo tags summary` output from Phase 1. Look for near-duplicates:
singular/plural (`filter`/`filters`), hyphenation variants (`bugfix`/`bug-fix`),
abbreviations (`perf`/`performance`). The canonical form should be the one used by
more files.

### Task completion vs status mismatch
```bash
# Completed items with unchecked tasks — systemic or one-off?
hyalo find --view completed-with-todos --index .hyalo-index --jq 'map({file, open: ([.tasks[] | select(.status != "x")] | length), total: (.tasks | length)})'
```
If many completed items have unchecked tasks, this is a workflow pattern — note it once
in the report rather than listing every file.

## Phase 4 — Consolidate

Fix what you can. Be conservative — prefer fixing metadata over deleting files. For
each change, note what you did and why.

**Keep using `--index .hyalo-index`** for all mutations — hyalo now patches the index
in-place after each file write, so it stays current for subsequent queries. No need to
drop the index before making changes. Only drop it at the very end (Phase 5).

### Fix broken links
Use `hyalo links fix` to auto-repair broken links. It uses fuzzy matching to find the
correct target (handles moves to `done/`, case changes, extension mismatches, etc.).

```bash
# Preview what will be fixed
hyalo links fix --format text --index .hyalo-index

# Apply fixes
hyalo links fix --apply --format text --index .hyalo-index
```

Review the dry-run output first. For any links it can't resolve (reported as unfixable),
leave them and report them in Phase 5.

### Update stale statuses
If an iteration's branch was merged:
```bash
hyalo set --property status=completed --file <path> --index .hyalo-index
```

If a backlog item's feature clearly shipped:
```bash
hyalo set --property status=completed --file <path> --index .hyalo-index
```

Only update when the evidence is clear. When uncertain, flag it in the report.

### Archive completed items
If completed items are in a top-level directory and a `done/` subfolder exists:
```bash
hyalo mv --file <old-path> --to <done-subdir/filename> --dry-run --index .hyalo-index
```
Review the dry-run output. If correct, execute without `--dry-run`.

### Normalize tags
```bash
hyalo tags rename --from <variant> --to <canonical> --index .hyalo-index
```

### Add missing cross-references
If a backlog item was implemented by an iteration but neither links to the other,
add a `[[wikilink]]`. Only where the relationship is clear and useful.

## Phase 5 — Report

Summarize everything. Structure as:

### Changes made
One line per change with reasoning:
```
- Set status=completed on iteration-43.md (branch iter-43/data-quality merged in abc1234)
- Moved iteration-46.md to iterations/done/ (completed, all tasks verified)
- Renamed tag: bugfix → bug-fix (2 files, matching existing convention)
- Fixed 5 broken links in research/dogfooding-v0.4.1-consolidated.md (same-dir targets)
```

### Issues requiring human attention
Things you detected but couldn't (or shouldn't) fix unilaterally. Keep it concise —
one line per issue with enough context to act on.

### KB health dashboard
Re-run `hyalo summary --format text` (fresh scan after mutations — no `--index`) and
compare with Phase 1 baseline. Report the delta: statuses changed, links fixed, tags
normalized, files moved.

## Ground rules

- **Conservative by default**: when in doubt, report rather than change.
- **Never delete files or body content**: update frontmatter, fix links, move files,
  suggest changes — but the user decides what to throw away.
- **Explain every change**: include the evidence (commit hash, task counts, etc.).
- **Don't modify Claude's memory files**: report stale memories but don't edit them.
- **Use hyalo for mutations**: `hyalo set`/`remove` for frontmatter, `hyalo tags rename`
  for tags, `hyalo mv` for moves. Fall back to Edit only for body content (fixing
  wikilink text in prose, adding cross-reference lines).
- **Batch similar findings**: if 15 completed items have unchecked tasks, say that once
  with the count. The report should be scannable in 30 seconds.
- **Minimize disk scans**: use `--index .hyalo-index` for all queries and mutations.
  Mutations automatically patch the index in-place — no need to drop and recreate.
  Only drop the index at the very end when the session is complete.
