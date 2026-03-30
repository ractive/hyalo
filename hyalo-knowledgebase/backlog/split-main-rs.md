---
title: "Split main.rs (1906 lines) into smaller modules"
type: backlog
date: 2026-03-29
status: planned
origin: codebase review 2026-03-29
priority: medium
tags:
  - refactor
  - structure
  - ai-friendliness
---

## Problem

`main.rs` contains six distinct concerns in one file: CLI structs, sub-enums, help text constants (~500 lines), help filtering logic, config/format merging, and the full command dispatch (~400 lines). This makes it hard for AI agents to navigate and modify.

## Proposed decomposition

- `cli/args.rs` — `Cli`, `Commands`, and all sub-enums (the derive structs)
- `cli/help.rs` — `HELP_EXAMPLES`, `HELP_LONG`, `filter_examples()`, `filter_long_help()`
- `lib.rs` — expose `run(cli: Cli, config: Config) -> Result<()>` with config-merge, index-load, and command dispatch
- `main.rs` — shrinks to ~30 lines: parse args, call `lib::run()`, handle exit code

## Acceptance criteria

- [ ] `main.rs` is under 100 lines
- [ ] CLI arg structs in their own module
- [ ] Help text in its own module
- [ ] Dispatch logic callable from lib.rs (enables integration testing)
- [ ] All existing tests pass
