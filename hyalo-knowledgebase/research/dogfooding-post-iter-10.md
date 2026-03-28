---
date: 2026-03-21
status: superseded
tags:
- research
- cli
- ux
- llm
title: 'Dogfooding Report: Post-Iteration 10'
type: research
---

# Dogfooding Report: Post-Iteration 10

Systematic test of every hyalo command and subcommand against `./hyalo-knowledgebase/` (32 files).

## Commands Tested

| Command | Subcommand | Result |
|---------|-----------|--------|
| `summary` | (default) | Works well. Text and JSON both clear. `--recent N` works. |
| `properties` | `summary` | Clean tabular output. |
| `properties` | `list` | Works with `--file`, `--glob`, and bare (all files). |
| `property` | `read` | Works. Good error on missing property. |
| `property` | `find` | Works with and without `--value`. Glob scoping works. |
| `property` | `set` | Not tested (would mutate knowledgebase). |
| `property` | `remove` | Not tested (would mutate). |
| `property` | `add-to-list` | Not tested (would mutate). |
| `property` | `remove-from-list` | Not tested (would mutate). |
| `tags` | `summary` | Clean output. |
| `tags` | `list` | Works with `--glob` scoping. |
| `tag` | `find` | Works. |
| `tag` | `add` / `remove` | Not tested (would mutate). |
| `links` | (default) | Works. `--resolved` and `--unresolved` filters work. |
| `outline` | (default) | Excellent. Text output is the best format in the whole tool. |
| `tasks` | (default) | Works. `--done`, `--todo`, `--status` filters work. |
| `task` | `read` | Works. |
| `task` | `toggle` / `set-status` | Not tested (would mutate). |
| `--jq` | various | Works well. Powerful for extracting specific fields. |
| `--version` | | Shows `hyalo 0.1.0`. |

## What Works Well

1. **Help text is excellent.** The `--help` (long form) is one of the best CLI help pages I've seen. The cookbook section with examples, output shapes, and command reference is genuinely useful. The short `-h` is also clean.
2. **Subcommand help is also good.** `hyalo property --help` clearly describes each subcommand with scope, side effects, and when to use.
3. **Error messages are helpful.** The "line N is not a task" error includes a hint (`use hyalo tasks --file <path> to list all tasks`). The "property not found" error includes the file path. The `--jq` + `--format text` conflict gives a clear resolution.
4. **`--format text` is genuinely readable.** The summary, outline, tasks text outputs are well-formatted.
5. **`--jq` is powerful.** Being able to do `--jq '.tasks.total'` or `--jq '[.[] | select(.total > 0)]'` without piping to external jq is excellent for LLM agents.
6. **Glob scoping on find/list commands works intuitively.** `property find --name status --value completed --glob 'backlog/**/*.md'` does exactly what you'd expect.
7. **`--file` and `--glob` mutual exclusivity is properly enforced** with a clear error message.
8. **Exit codes are correct.** 1 for user errors, 2 for usage errors (clap).

## Issues and Improvement Ideas

### ISSUE-1: `tasks --todo` output is very noisy (files with 0 tasks shown)

When running `tasks --todo --format text`, all 32 files are listed even though only 5 have incomplete tasks. The output is dominated by `filename.md (0 tasks)` lines. This is the single most annoying thing about the tool right now.

**Suggestion:** When `--done` or `--todo` filter is active, suppress files with 0 matching tasks from text output. Or add a `--hide-empty` flag. The JSON output could also benefit from omitting empty entries (or have a flag for it).

**Workaround:** `--jq '[.[] | select(.total > 0)]'` works but defeats the purpose of `--format text`.

### ISSUE-2: `outline --file SEED.md` on a file with no frontmatter/content shows just the filename

Running `outline --file SEED.md --format text` on a nearly empty file outputs just `SEED.md` with nothing below it. This is technically correct but feels like it should say something like "(empty)" or "(no sections)".

### ISSUE-3: Unresolved link target `obsidian-properties` not helpful

`links --file research/obsidian-markdown-compatibility.md --unresolved` shows `obsidian-properties (unresolved)`. The file `research/obsidian-properties.md` exists, so this is the known shortest-path resolution limitation ([[backlog/done/shortest-path-link-resolution]]). The wikilink `[[obsidian-properties]]` should resolve to `research/obsidian-properties.md` but doesn't because hyalo requires the path prefix. This is already tracked as a backlog item.

### ISSUE-4: No way to search file body content

There is no `grep` or `search` command. If I want to find files mentioning "rayon" in the body text (not just frontmatter), I can't do it with hyalo. I have to use external grep. This is the biggest missing feature for a vault management tool.

**Suggestion:** A `hyalo search --query "rayon"` or `hyalo grep "rayon"` command that searches markdown body content (excluding frontmatter) and returns file paths + matching lines. This would complement the existing property/tag find commands. See [[backlog/done/combined-queries]].

### ISSUE-5: No way to view/read file content

Hyalo can outline a file, list its properties, list its tasks, and show its links. But there is no way to actually _read_ the file content. For an LLM agent workflow, you often want to see what a file says after finding it via `property find` or `tag find`.

**Suggestion:** A `hyalo read --file path.md` command that outputs the file body (or a section of it). Even just `hyalo read --file path.md --section "## Problem"` to read a specific heading's content would be powerful for LLM agents.

### ISSUE-6: `summary` text output doesn't show file list for each status

The text summary shows `Status: active (2), completed (13), ...` but doesn't tell you _which_ files. You have to switch to JSON or use `--jq` to see them. The JSON output includes the file lists, but text doesn't. This is a discoverability gap -- the text output is meant for humans, and a human would want to know "which 2 files are active?"

### ISSUE-7: `properties list` on a file with no frontmatter shows just the filename

`properties list --file SEED.md` shows just `SEED.md` with a blank line. Could show "(no properties)" or similar.

### ISSUE-8: `tasks --todo` on superseded iterations is confusing

The superseded iterations (iteration-08-tasks.md, iteration-3b-tasks.md) show 31-32 incomplete tasks each. These are never going to be completed because the iterations were superseded. There's no way to filter by frontmatter status when listing tasks, e.g., "show me todo tasks only in files where status != superseded".

**Suggestion:** Allow combining `tasks` with property filters, e.g., `tasks --todo --where status=completed` or integrate with [[backlog/done/combined-queries]]. Alternatively, the workaround is `tasks --todo --glob 'iterations/iteration-09*'` to target specific files, which already works.

### IDEA-1: Pipe-friendly file list output

Several commands output file lists (property find, tag find). It would be nice if there was a mode optimized for piping, e.g., NUL-delimited output for `xargs -0`. The cookbook shows `--jq '.files[]' | xargs -I{} ...` which works but is verbose.

### IDEA-2: (Retracted) `summary --glob` already works

Initially thought summary didn't support `--glob`, but it does! `summary --glob 'backlog/*.md'` correctly scopes the summary to just backlog items. Well done.

## Workflow Observations

### Workflow: "What's the current state of the project?"
`summary --format text` is the perfect entry point. Clean, fast, informative. This is the single best command.

### Workflow: "Find all incomplete work"
`tasks --todo --format text` works but is noisy (ISSUE-1). The jq workaround is adequate for JSON consumers but bad for text-mode humans.

### Workflow: "Drill down from summary to details"
Summary shows "Status: planned (2)". To find out which: `property find --name status --value planned`. This works but requires knowing the command tree. The [[backlog/done/discoverable-drill-down-commands]] backlog item would help here.

### Workflow: "What changed recently?"
Summary shows recent files. But to see _what_ changed, you'd need to read the file. No read command exists (ISSUE-5).

### Workflow: "Find files about a topic"
Tag find works if the topic is tagged. But if the topic is only mentioned in body text, there's no way to find it (ISSUE-4).

## Overall Assessment

Hyalo is a solid, well-designed tool for its scope. The help system is best-in-class. Error messages are helpful and actionable. The JSON + jq integration is excellent for LLM agents. The text output is clean and readable.

The biggest gaps for a "second brain" tool are: (1) no body content search, (2) no file read command, and (3) noisy output for filtered task lists. These are the things that would make the biggest difference for daily use.

For LLM agent use specifically, the tool is already quite good. The JSON output shapes are consistent and well-documented. The `--jq` flag eliminates the need for external tooling. The main thing missing is the ability to read file content after finding a file.
