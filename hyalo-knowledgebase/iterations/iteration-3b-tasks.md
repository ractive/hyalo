---
title: "Iteration 3b — Task Commands"
type: iteration
date: 2026-03-20
status: planned
branch: iter-3b/task-commands
tags:
  - iteration
  - tasks
  - scanner
---

# Iteration 3b — Task Commands

## Goal

Provide CLI commands to list, inspect, and modify markdown tasks (checkboxes) within individual files. Tasks are always file-scoped (`--file` required) — vault-wide task search is deferred to the indexing iteration. Support reading task status, toggling completion, and setting custom status characters.

## CLI Interface

```sh
# List tasks in a file
hyalo tasks --file FILE [--status CHAR] [--done] [--todo] [--dir DIR] [--format json|text]

# Show a single task
hyalo task read --file FILE --line N [--dir DIR] [--format json|text]

# Toggle task completion ([ ] ↔ [x])
hyalo task toggle --file FILE --line N [--dir DIR] [--format json|text]

# Set custom status character
hyalo task set-status --file FILE --line N --status CHAR [--dir DIR] [--format json|text]
```

### Output Examples

**`hyalo tasks --file iterations/iteration-01-frontmatter-properties.md --todo`** (JSON):
```json
{
  "file": "iterations/iteration-01-frontmatter-properties.md",
  "tasks": [
    { "line": 43, "status": " ", "text": "Implement streaming line scanner" },
    { "line": 44, "status": " ", "text": "Track fenced code block state" }
  ],
  "total": 2
}
```

**`hyalo task read --file plan.md --line 12`** (JSON):
```json
{
  "file": "plan.md",
  "line": 12,
  "status": " ",
  "text": "Write integration tests",
  "done": false
}
```

**`hyalo task toggle --file plan.md --line 12`** (JSON):
```json
{
  "file": "plan.md",
  "line": 12,
  "status": "x",
  "text": "Write integration tests",
  "done": true
}
```

### Task Syntax

A task is a line matching the pattern: optional whitespace, then `- [C] ` where `C` is any single character. Examples:

- `- [ ] incomplete task` — status `" "`, done = false
- `- [x] completed task` — status `"x"`, done = true
- `- [-] cancelled task` — status `"-"`, done = false
- `- [?] question` — status `"?"`, done = false
- `- [!] important` — status `"!"`, done = false

Only `[x]` and `[X]` are considered "done". All other status characters are "not done".

### Filter Flags

- `--todo` — show tasks where done = false (any status except `x`/`X`)
- `--done` — show tasks where done = true (status `x` or `X`)
- `--status CHAR` — show tasks with exactly this status character (e.g. `--status "-"` for cancelled)
- Filters are mutually exclusive. Using multiple is an error.

### Behavior Notes

- `--file` is required for all task commands (no vault-wide mode, see [[decision-log#DEC-021]])
- `task toggle` flips `[ ]` → `[x]` and `[x]` / `[X]` → `[ ]`. For custom statuses like `[-]`, toggle sets to `[x]` (marking done)
- `task set-status` accepts any single character
- Line numbers are 1-based (matching editor conventions)
- Task mutation edits the file in-place, preserving all other content
- If the specified line is not a task, commands return a user error with hint

## New Modules

### `src/tasks.rs` — Task Extraction & Mutation

Task parsing uses the existing streaming scanner to find task lines. Task mutation reads the full file, modifies the target line, and writes back (similar to frontmatter mutation but operating on body lines).

**Key types:**
```rust
pub struct Task {
    pub line: usize,       // 1-based line number
    pub status: char,      // the character between [ and ]
    pub text: String,      // task text after `] `
    pub done: bool,        // true only for 'x' or 'X'
}
```

### `src/commands/tasks.rs` — Command Implementations

## Tasks

### Task Parsing
- [ ] Implement task line regex/pattern matching: `- [C] text`
- [ ] Extract tasks from a file using the streaming scanner
- [ ] Handle indented tasks (nested lists)
- [ ] Unit tests for task parsing (various status chars, indentation, edge cases)

### Task Mutation
- [ ] Implement in-place line editing: read file, modify target line, write back
- [ ] `toggle`: `[ ]` → `[x]`, `[x]`/`[X]` → `[ ]`, custom → `[x]`
- [ ] `set-status`: replace status character
- [ ] Validate that target line is actually a task before mutation
- [ ] Unit tests for mutation logic

### Commands
- [ ] `tasks` command — list tasks with optional filters (`--done`, `--todo`, `--status`)
- [ ] `task read` command — show single task by file + line
- [ ] `task toggle` command — toggle completion state
- [ ] `task set-status` command — set custom status character
- [ ] Wire up CLI in main.rs with clap subcommands
- [ ] Validate filter flag mutual exclusivity

### Testing
- [ ] E2E tests for `tasks --file` (all tasks, `--done`, `--todo`, `--status`)
- [ ] E2E tests for `task read` (valid line, non-task line, out-of-range line)
- [ ] E2E tests for `task toggle` (incomplete → done, done → incomplete, custom → done)
- [ ] E2E tests for `task set-status`
- [ ] E2E tests for error cases (missing `--file`, invalid line number, line is not a task)

### Quality Gates
- [ ] `cargo fmt`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`

### Dogfooding
- [ ] `hyalo tasks --file iterations/iteration-01-frontmatter-properties.md --dir hyalo-knowledgebase` — list tasks in iteration 1
- [ ] `hyalo task read --file iterations/iteration-01-frontmatter-properties.md --line 43 --dir hyalo-knowledgebase`

## Acceptance Criteria

- [ ] `hyalo tasks --file FILE` lists all tasks with line, status, text, done
- [ ] `--done`, `--todo`, `--status CHAR` filters work correctly and are mutually exclusive
- [ ] `hyalo task read --file FILE --line N` shows task details
- [ ] `hyalo task toggle --file FILE --line N` toggles completion state
- [ ] `hyalo task set-status --file FILE --line N --status CHAR` sets custom status
- [ ] Non-task lines return user error with helpful hint
- [ ] All quality gates pass: `cargo fmt && cargo clippy && cargo test`
