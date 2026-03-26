---
name: hyalo-dream
description: >
  Perform a reflective consolidation pass over a hyalo-managed knowledgebase directory —
  detecting structural issues, fixing broken links, flagging stale content, normalizing
  metadata, and reporting what changed. Use this skill when the user says /hyalo-dream,
  "consolidate the knowledgebase", "clean up the KB", "run KB hygiene", "dream", or
  "what needs attention in the knowledgebase". Also use when the user asks about
  knowledgebase health, broken links, orphan files, stale iterations, or metadata
  inconsistencies.
context: fork
disable-model-invocation: true
---

# Hyalo Dream — Knowledgebase Consolidation

You are performing a dream — a reflective pass over a knowledgebase. Your job is to
detect issues, fix what you can, and report what needs human attention. Think of this
as a librarian doing a periodic shelf-read: checking that everything is filed correctly,
cross-references work, and nothing is gathering dust in the wrong place.

This process has 5 phases. Take your time — a thorough dream is worth more than a fast
one. A few minutes is fine.

## Before you start

Locate the hyalo binary:
```bash
which hyalo 2>/dev/null || echo "target/release/hyalo"
```

Confirm `.hyalo.toml` exists in the project root to determine the KB directory. If it
doesn't exist, ask the user which directory to consolidate.

## Phase 1 — Orient and snapshot

Get the lay of the land with two commands. The first gives you the overview, the second
gives you the full dataset you'll query throughout the rest of the dream.

```bash
# 1. High-level overview (baseline for the final health dashboard)
hyalo summary --format text

# 2. Full snapshot — one scan that captures everything. Save to a temp file so you can
#    run many jq filters without re-scanning the filesystem each time.
HYALO_DREAM_SNAPSHOT="$(mktemp "${TMPDIR:-/tmp}/hyalo-dream-snapshot.XXXXXX.json")"
hyalo find --fields properties,tags,sections,tasks,links,backlinks > "${HYALO_DREAM_SNAPSHOT}"
```

The snapshot contains every file's properties, tags, sections, tasks, links, and
backlinks. All detection queries in Phase 3 should use `jq` on this file rather than
running separate `hyalo find` calls. This turns ~15 disk scans into 1.

**Important:** All subsequent `jq` commands in this skill reference `${HYALO_DREAM_SNAPSHOT}`
instead of a hardcoded path.

Also grab the tag vocabulary for inconsistency detection:
```bash
hyalo tags summary --format text
```

## Phase 2 — Gather recent signal

Before looking for issues, understand what happened recently. This context is what
makes the dream valuable — it tells you what *should* have changed in the KB, so you
can spot what didn't.

### Git history

```bash
# What branches were merged recently? What iteration branches shipped?
git log --oneline --merges --since="4 weeks ago"

# What areas of code changed? (helps identify which features shipped)
git log --oneline --since="4 weeks ago" -- "*.rs" | head -30
```

Extract non-completed iterations and their branches from the snapshot:
```bash
jq 'map(select(.properties.type == "iteration" and .properties.status != "completed" and .properties.status != "superseded" and .properties.status != "wont-do")) | map({file, branch: .properties.branch, status: .properties.status})' ${HYALO_DREAM_SNAPSHOT}
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

If found, query it with hyalo:
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

All queries below run on the cached snapshot — no additional disk scans needed.

### Broken links
```bash
jq '[.[] | {file: .file, broken: [.links[] | select(.path == null)] | map(.target)} | select(.broken | length > 0)]' ${HYALO_DREAM_SNAPSHOT}
```
For each broken link, try to find the intended target:
- Search for the filename in `done/` subdirectories (common after archiving)
- If the target is in the same directory as the source, try with the directory prefix
- Check if the target exists under a slightly different name

Categorize as: **resolvable** (target exists elsewhere) vs **truly broken** (doesn't
exist at all).

### Orphan files
```bash
jq 'map(select(.backlinks | length == 0)) | map(.file)' ${HYALO_DREAM_SNAPSHOT}
```
Not all orphans are problems. Expect these to be legitimately orphaned:
- Top-level files (SEED.md, project-pitch.md, decision-log.md)
- Research documents (standalone reports)
- Older completed items in `done/` directories

Focus on **actionable orphans**: active/planned items that should be cross-referenced.

### Stale statuses
```bash
# In-progress items — should any be completed?
jq 'map(select(.properties.status == "in-progress")) | map({file, date: .properties.date, branch: .properties.branch})' ${HYALO_DREAM_SNAPSHOT}

# Planned items where all tasks are done
jq 'map(select(.properties.status == "planned" and (.tasks | length > 0) and ([.tasks[] | select(.status != "x")] | length) == 0)) | map(.file)' ${HYALO_DREAM_SNAPSHOT}

# In-progress items sorted by date (oldest first — possibly stale)
jq 'map(select(.properties.status == "in-progress" and .properties.date != null)) | sort_by(.properties.date) | map({file, date: .properties.date})' ${HYALO_DREAM_SNAPSHOT}
```
Cross-reference with git merges from Phase 2. If the branch was merged, update status.

### Stale backlog items
```bash
jq 'map(select(.properties.status == "planned" and .properties.type == "backlog")) | map({file, title: .properties.title})' ${HYALO_DREAM_SNAPSHOT}
```
Compare each planned backlog item against merged iterations and recent git history.
If the feature clearly shipped (in a different iteration or under a different name),
flag it.

### Missing metadata
```bash
jq 'map(select(.properties.status == null)) | map(.file)' ${HYALO_DREAM_SNAPSHOT}
jq 'map(select(.properties.type == null)) | map(.file)' ${HYALO_DREAM_SNAPSHOT}
```

### Tag inconsistencies
Review the `hyalo tags summary` output from Phase 1. Look for near-duplicates:
singular/plural (`filter`/`filters`), hyphenation variants (`bugfix`/`bug-fix`),
abbreviations (`perf`/`performance`). The canonical form should be the one used by
more files.

### Task completion vs status mismatch
```bash
# Completed items with unchecked tasks — systemic or one-off?
jq 'map(select(.properties.status == "completed" and (.tasks | length > 0) and ([.tasks[] | select(.status != "x")] | length) > 0)) | map({file, open: ([.tasks[] | select(.status != "x")] | length), total: (.tasks | length)})' ${HYALO_DREAM_SNAPSHOT}
```
If many completed items have unchecked tasks, this is a workflow pattern — note it once
in the report rather than listing every file.

## Phase 4 — Consolidate

Fix what you can. Be conservative — prefer fixing metadata over deleting files. For
each change, note what you did and why.

### Fix broken links
For resolvable broken links (target moved to `done/` etc.), update the wikilink text
in the source file. Use the snapshot to confirm the correct path, then Edit the source
file.

For truly broken links (target never existed), leave them and report them.

### Update stale statuses
If an iteration's branch was merged:
```bash
hyalo set --property status=completed --file <path>
```

If a backlog item's feature clearly shipped:
```bash
hyalo set --property status=completed --file <path>
```

Only update when the evidence is clear. When uncertain, flag it in the report.

### Archive completed items
If completed items are in a top-level directory and a `done/` subfolder exists:
```bash
hyalo mv --file <old-path> --to <done-subdir/filename> --dry-run
```
Review the dry-run output. If correct, execute without `--dry-run`.

### Normalize tags
```bash
hyalo tags rename --from <variant> --to <canonical>
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
Re-run `hyalo summary --format text` and compare with Phase 1 baseline. Report the
delta: statuses changed, links fixed, tags normalized, files moved.

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
- **Minimize disk scans**: use the `${HYALO_DREAM_SNAPSHOT}` for all read queries.
  Only call `hyalo find` again if you need data not in the snapshot.
