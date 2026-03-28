---
branch: iter-52/subcommand-suggestions
date: 2026-03-27
status: completed
tags:
- cli
- ux
- error-handling
- llm
title: Subcommand Suggestion on Misplaced Flags
type: iteration
---

# Iteration 52 — Subcommand Suggestion on Misplaced Flags

## Problem

LLMs (and humans) frequently place flags before the subcommand name:

```
hyalo task --file foo.md --toggle --line 28
```

Clap rejects this with an unhelpful `error: unexpected argument '--toggle' found`.
The user must figure out the correct ordering themselves.

## Goal

When clap rejects an unknown flag that matches a known subcommand name, emit a
corrected command suggestion that the user (or LLM) can copy-paste directly:

```
error: unexpected argument '--toggle' found

  tip: 'toggle' is a subcommand, not a flag. Did you mean:

    hyalo task toggle --file foo.md --line 28
```

## Approach

Intercept clap parsing errors using `try_parse()` / `try_get_matches_from()`.
On `ErrorKind::UnknownArgument`, check if stripping `--` from the invalid arg
yields a known subcommand name. If so, reconstruct the corrected command line
and inject a `ContextKind::Suggested` tip into the error before printing.

This requires **no changes to the command structure** — the fix is entirely in
error handling, making it low-risk.

## Use Cases

### UC1: Flag-style subcommand (`--toggle` → `toggle`)
```
Input:  hyalo task --file foo.md --toggle --line 28
Output: tip: ... Did you mean: hyalo task toggle --file foo.md --line 28
```

### UC2: Flag-style subcommand for properties (`--rename`)
```
Input:  hyalo properties --rename --from old --to new
Output: tip: ... Did you mean: hyalo properties rename --from old --to new
```

### UC3: Flag-style subcommand for tags (`--summary`)
```
Input:  hyalo tags --summary
Output: tip: ... Did you mean: hyalo tags summary
```

### UC4: Hyphenated subcommand (`--set-status`)
```
Input:  hyalo task --file foo.md --set-status --line 28 --status ?
Output: tip: ... Did you mean: hyalo task set-status --file foo.md --line 28 --status ?
```

### UC5: Interleaved flags and subcommand
```
Input:  hyalo task --file foo.md --line 28 --toggle
Output: tip: ... Did you mean: hyalo task toggle --file foo.md --line 28
```

### UC6: Unknown flag that does NOT match a subcommand (no false positive)
```
Input:  hyalo task --verbose --file foo.md toggle
Output: (clap's default error, no subcommand suggestion)
```

### UC7: Misspelled subcommand as flag (`--togle`)
Stretch goal — if `--togle` is close enough to `toggle` (fuzzy match), suggest it.
Keep this optional; exact match is the priority.

## Test Cases

- [x] `hyalo task --toggle --file f --line 1` → suggests `hyalo task toggle --file f --line 1`
- [x] `hyalo task --file f --line 1 --toggle` → same suggestion (flag at end)
- [x] `hyalo task --file f --toggle --line 1` → same suggestion (flag in middle)
- [x] `hyalo task --set-status --file f --line 1 --status ?` → suggests `hyalo task set-status ...`
- [x] `hyalo properties --rename --from a --to b` → suggests `hyalo properties rename ...`
- [x] `hyalo properties --summary` → suggests `hyalo properties summary`
- [x] `hyalo tags --rename --from a --to b` → suggests `hyalo tags rename ...`
- [x] `hyalo tags --summary` → suggests `hyalo tags summary`
- [x] `hyalo task --read --file f --line 1` → suggests `hyalo task read ...`
- [x] `hyalo task --verbose --file f toggle` → no subcommand suggestion (--verbose is genuinely unknown)
- [x] `hyalo task toggle --file f --line 1` → parses normally, no error
- [x] Suggestion reconstructs full corrected command including all other flags
- [x] Works with short flags: `hyalo task --toggle -f foo.md -l 28` → correct suggestion
- [x] Error exits with non-zero status code
- [x] `--help` and `--version` still work normally

## Tasks

- [x] Switch `cmd.get_matches()` to `Cli::try_parse()` in main.rs
- [x] Extract subcommand names from the `Command` tree for parent commands that have subcommands
- [x] On `ErrorKind::UnknownArgument`, strip `--` prefix and check against subcommand names
- [x] Reconstruct corrected command: move subcommand name to correct position, keep all other args
- [x] Inject `ContextKind::Suggested` tip into the clap error with the corrected command
- [x] Fall through to default clap error for non-matching cases
- [x] Add unit tests for command reconstruction logic
- [x] Add e2e tests covering all test cases above
- [x] Run fmt, clippy, test gates

## Non-Goals

- Changing the command structure (no arg lifting/flattening)
- Supporting arbitrary reordering of all args
- Fuzzy matching for misspelled subcommands (stretch goal only)

## References

- [[iterations/done/iteration-49-init-improvements]] — last iteration touching CLI parsing
- clap `ErrorKind::UnknownArgument` + `ContextKind::Suggested`
- clap `try_get_matches_from()` API
