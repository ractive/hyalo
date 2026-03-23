---
title: "Dogfooding Report: Post-Iteration 19 — Knowledgebase Cleanup"
type: research
date: 2026-03-23
status: completed
tags:
  - research
  - cli
  - ux
  - llm
  - dogfooding
---

# Dogfooding Report: Post-Iteration 19 — Knowledgebase Cleanup

Task: clean up the knowledgebase — find outdated backlog items, stale research, inconsistent metadata. All exploration done via hyalo CLI.

## Hyalo Commands Used (in order)

| # | Command | Purpose | Outcome |
|---|---------|---------|---------|
| 1 | `hyalo summary --format text` | Orient: file counts, status distribution, recent files | Worked perfectly. Great entry point. |
| 2 | `hyalo find --glob 'backlog/*.md' --format text` | List active backlog items | Clean output, showed all metadata |
| 3 | `hyalo find --glob 'backlog/done/*.md' --format text` | List completed backlog items | Good |
| 4 | `hyalo find --glob 'research/*.md' --format text` | List research files | Good |
| 5 | `hyalo find --glob 'iterations/*.md' --fields properties --format text` | List iterations with status | Useful for seeing all 19 iterations at a glance |
| 6 | `hyalo find --glob '*.md' --format text` | List root-level files | Good |
| 7 | `hyalo find "combined queries" --fields properties,sections --format text` | Inspect a specific backlog item | Body text search + field projection worked well |
| 8 | `hyalo set --property status=completed --file backlog/combined-queries.md` | Mark backlog item as done | Clean mutation output |
| 9 | `hyalo set --property status=completed --file research/cli-discoverable-drill-down-commands.md` | Mark research as completed | Good |
| 10 | `hyalo set --property status=completed --file research/cli-structured-output-patterns.md` | Mark research as completed | Good |
| 11 | `hyalo set --property status=superseded --file research/dogfooding-post-iter-10.md` | Mark old dogfooding report | Good |
| 12 | `hyalo set --property status=reference --file X --file Y --file Z` | Batch-set 3 Obsidian reference docs | **FAILED** — `--file` can't be repeated. Had to use `--glob 'research/obsidian-*.md'` instead |
| 13 | `hyalo set --property status=reference --glob 'research/obsidian-*.md'` | Workaround for above | Worked — 3/3 modified |
| 14 | `hyalo set --property status=completed --file research/performance-benchmarking.md` | Mark as completed | Good |
| 15 | `hyalo set --property status=completed --file research/performance-parallelization.md` | Mark as completed | Good |
| 16 | `hyalo set --property status=deferred --file iterations/iteration-13-read-command.md` | Mark skipped iteration | Good |
| 17 | `hyalo summary --format text` | Verify cleanup results | Good — status distribution updated |
| 18 | `hyalo find --property status=planned` | Check remaining planned items | Found 1 (sqlite-indexing) — correct |
| 19 | `hyalo find --property status=done` | Find inconsistent status values | Found 1 file using "done" instead of "completed" |
| 20 | `hyalo set --property status=completed --file backlog/done/comment-block-handling.md` | Normalize status | Good |
| 21 | `hyalo find --property status=active --fields properties` | Verify active files | 2 files — plan + pitch, both correct |

## What Worked Well

1. **`summary` is the perfect starting point.** Status distribution immediately showed the cleanup opportunities (planned items that were done, inconsistent status values).
2. **`find --property status=X` is powerful for auditing.** Quickly found all "planned", "done", "active" files to verify correctness.
3. **`find --glob` scoping is intuitive.** `backlog/*.md` vs `backlog/done/*.md` vs `research/*.md` — easy to explore by folder.
4. **`set --property` mutations are clean.** Output clearly shows what was modified. The "1/1 modified" confirmation is reassuring.
5. **`--format text` is readable.** The output is easy to scan for an LLM agent building a picture of the vault.
6. **Body text search via positional pattern works.** `find "combined queries"` found the right file instantly.

## Issues Found

### ISSUE-1: `set` doesn't support multiple `--file` flags

Tried `hyalo set --property status=reference --file A --file B --file C` — got an error: "the argument '--file <FILE>' cannot be used multiple times". Had to use `--glob` as a workaround. But sometimes the files you want to update aren't glob-matchable (e.g., 3 specific files in different folders).

**Suggestion:** Either allow `--file` to be repeated, or add a `--files` flag that accepts multiple paths. This would also benefit `find` for targeting specific files.

### ISSUE-2: No way to move files via hyalo

Had to use `mv` to move `combined-queries.md` from `backlog/` to `backlog/done/`. A `hyalo move` command would be useful for maintaining wikilink integrity (though in this case there were no inbound links).

### ISSUE-3: No bulk status normalization

Found one file using `status: done` instead of `status: completed`. No easy way to say "change all `done` to `completed`" in one command. Had to find the files, then set each one.

**Workaround:** Could script it: `hyalo find --property status=done --jq '.[].file' | xargs -I{} hyalo set --property status=completed --file {}` — this works but feels like a common enough pattern to deserve a built-in.

### ISSUE-4: Can't read file body content

When auditing the dogfooding report (research/dogfooding-post-iter-10.md), I needed to read the actual content to decide if issues were resolved. Hyalo can show sections/properties/tasks but not the body text. Had to fall back to the Read tool. This is the same gap identified in the post-iter-10 dogfooding report — iteration 13 (read command) is still unimplemented.

## Feature Ideas

1. **`--file` should be repeatable** across `find`, `set`, `remove`, `append` — target specific files without a glob
2. **`hyalo move --file old --to new`** — move/rename with wikilink updates
3. **`hyalo find --property status=done --set status=completed`** — inline mutation on find results (like `sed -i` for frontmatter)
4. **`hyalo read --file X`** — display file body content (iter-13)
5. **Status value validation/normalization** — warn when a file uses a status value that only appears once (likely a typo or inconsistency)

## Overall Assessment

Hyalo handled this cleanup task well. The `summary → find → set` workflow is natural and efficient. The 21 commands above covered the entire cleanup task. The main friction points were (1) needing `mv` for file moves and (2) needing external `Read` for file content. Both are known gaps.

For LLM agent use, hyalo is excellent for metadata-level operations. The JSON output + `--jq` pipeline is powerful. The text output is clean and scannable. The main limitation is that it's a metadata tool — when you need to reason about file *content*, you still need to read the file externally.
