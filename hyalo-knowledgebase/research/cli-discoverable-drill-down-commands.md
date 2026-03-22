---
title: "CLI Discoverable Drill-Down Commands"
type: research
date: 2026-03-21
tags: [cli, ux, discoverability, progressive-disclosure, command-suggestion]
---

# CLI Discoverable Drill-Down Commands

Research into how CLI tools implement "discoverable drill-down commands" -- suggesting next actions or related commands based on context.

## 1. Core Design Patterns

### Progressive Disclosure
The most widely recognized pattern. Originally a GUI concept (coined by Jakob Nielsen), it means showing only what the user needs now and revealing more on demand. In CLI context:
- `--help` shows basic usage; `--help <subcommand>` shows details
- Short output by default; `--verbose` or `--detail` for more
- AWS CLI wizards: `aws lambda wizard new-function` presents an interactive form, one parameter at a time, then previews the full CLI command or CloudFormation template
- macOS print dialog is the canonical GUI example (basic options with "Show Details" button)

### Contextual Next-Step Suggestions
After a command completes, the tool prints suggested follow-up commands. This is the most directly relevant pattern.

**Git examples:**
- After `git init`: "use `git add` to track files"
- After `git checkout -b`: "use `git push -u origin <branch>` to publish"
- After `git commit`: "use `git push` to publish your local commits"
- On detached HEAD: "use `git switch -` to return to previous branch"
- On merge conflict: "fix conflicts and run `git commit`"
- After `git clone`: suggests `cd <repo>`

**GitHub CLI (`gh`) examples:**
- After `gh pr create`: shows "What's next?" menu with options to open in browser, add reviewers, etc.
- `gh run watch` without args: prompts selection from recent runs
- Interactive flows loop back to "What's next?" after each action

**Docker/kubectl:**
- `docker build` suggests `docker run` with the built image
- kubectl provides "cheat sheet" style cross-references in help text

### "Did You Mean?" Typo Correction
When a user enters an unknown command, suggest the closest valid one.

**Git implementation:**
- Uses Damerau-Levenshtein distance (in `levenshtein.c`)
- `help.c` loads all valid subcommands + aliases, computes distances, suggests closest match
- `help.autocorrect` config: can auto-execute after N deciseconds delay
- Only keeps last 3 rows of distance matrix in memory (space optimization)

**TheFuck:**
- Matches failed command against a library of "rules" (Python modules)
- Each rule has `match(command)` and `get_new_command(command)` functions
- Rules are community-contributed; covers git, docker, npm, cargo, etc.
- Works retroactively on the *previous* failed command

### Wizard / Guided Flows
Interactive step-by-step command construction.

**AWS CLI Wizards:**
- `aws <service> wizard <wizard-name>` opens an interactive form
- Parameters presented one at a time: either selection list or text input
- Preview mode shows the equivalent CLI command (teaches the non-wizard syntax)
- Available for Lambda, EventBridge, IAM, DynamoDB, etc.

**`gh pr create` interactive mode:**
- Prompts for title, body, reviewers, labels step by step if not provided as flags
- Falls back to editor for body text

### Cheatsheet / Example-Based Discovery
**tldr:** Community-maintained simplified man pages with practical examples only. `tldr tar` shows the 5 most common usage patterns.

**navi:** Interactive cheatsheet browser. Displays parameter suggestions dynamically as you type. Integrates with tldr-pages and cheat.sh. Fuzzy-searches across all cheatsheets.

## 2. Architectural Approaches

### Static Hint Tables
The simplest approach. Each command has a hardcoded list of "next commands" or "see also" references.
```
CommandHints {
    "init" => ["add", "config"],
    "add"  => ["commit", "status"],
    "commit" => ["push", "log"],
}
```
Pros: trivial to implement, zero runtime cost. Cons: no awareness of actual state.

### State-Aware / Context-Sensitive Hints
The tool inspects the current state and tailors suggestions accordingly.

**Git's approach:**
- Checks repo state (merge in progress? detached HEAD? upstream configured?)
- Different hints for `status` depending on whether files are staged, unstaged, untracked
- Post-operation hints depend on what just happened

**Implementation pattern:**
```
fn suggest_next(context: &AppState, last_command: &str) -> Vec<Suggestion> {
    match (last_command, context) {
        ("search", ctx) if ctx.has_results() => vec!["show <id>", "filter --tag"],
        ("show", _) => vec!["edit", "delete", "list"],
        _ => vec!["help"],
    }
}
```

### Graph-Based / DAG Workflows
Model the command space as a directed graph where edges represent natural workflows.
- Nodes = commands/states
- Edges = transitions with preconditions
- At any node, outgoing edges are the suggested next commands
- Can be weighted by frequency or relevance

This is analogous to how workflow engines work, but applied to CLI UX. LangGraph (for AI agents) uses exactly this pattern: nodes + edges with a state machine loop.

### Rule-Based Suggestion Engine
Like TheFuck but proactive rather than reactive:
- Rules match on (last_command, exit_code, output, state)
- Each rule produces suggested commands
- Rules can be prioritized/weighted
- Community-extensible via plugin system

## 3. Relevant Rust Crates

### For String Similarity ("Did You Mean?")
- **strsim** (crates.io/crates/strsim): Levenshtein, Damerau-Levenshtein, Jaro-Winkler, Sorensen-Dice. The standard choice.
- **fuzzt**: Built on strsim, adds Top-N matching from a collection.
- **fuzzy-search**: BK Trees + Levenshtein Automaton for large datasets.

### For Interactive Prompts (Wizard Flows)
- **dialoguer**: Confirmation, text input, select, multi-select, password prompts. Mature and widely used.
- **inquire**: More prompt types (date select, editor, custom type parsing). Supports autocompletion in text prompts.
- **cliclack**: Newer, visually polished prompts.

### For CLI Framework (Built-in Suggestion Support)
- **clap**: Already has "did you mean?" for misspelled subcommands and flags. `clap_complete` generates shell completions. `ValueHint` provides semantic hints (file path, URL, etc.).
- **clap-repl**: Adds REPL mode to clap CLIs with autocompletion and suggestions.

### For Output Formatting (Rendering Hints)
- **console** + **indicatif**: Terminal styling and progress bars.
- **owo-colors** or **colored**: ANSI color output for distinguishing hints from regular output.

## 4. Design Guidelines from clig.dev

The [Command Line Interface Guidelines](https://clig.dev/) recommend:
- **Suggest next steps**: "When several commands form a workflow, suggesting to the user commands they can run next helps them learn how to use your program and discover new functionality."
- **Suggest fixes on error**: Don't just say "invalid"; say what to do instead.
- **Be discoverable**: "Discoverable CLIs have comprehensive help texts, provide lots of examples, suggest what command to run next, and suggest what to do when there is an error."
- **Limit nesting to 2 levels**: Temporal's CLI proposal found that deeper nesting hurts discoverability.
- **Provide examples in help text**: Real, copy-pasteable examples, not abstract syntax diagrams.

## 5. Temporal CLI Discoverability Proposal

Temporal's [proposal](https://github.com/temporalio/proposals/blob/master/cli/000-cli-improve-commands-discoverability.md) identified these problems and solutions:
- **Problem**: Deep nesting (3+ levels) makes commands hard to discover
- **Problem**: Inconsistent command naming hinders guessability
- **Solution**: Max 2 levels of nesting
- **Solution**: Consistent verb-noun patterns (`workflow list`, `workflow describe`)
- **Solution**: Consistent flag names across commands
- **Principle**: Users should be able to *guess* commands based on patterns they've learned

## 6. Concrete Implementation Ideas for Hyalo

Based on this research, several patterns could apply:

1. **Post-command hints**: After `hyalo search`, suggest `hyalo show <id>` or `hyalo edit <file>`. After `hyalo init`, suggest `hyalo new`.

2. **"Did you mean?" via strsim**: If user types `hyalo serach`, suggest `hyalo search`. Clap may already handle subcommand typos.

3. **Context-aware suggestions**: After showing a note, suggest related notes via wikilinks or tags. After listing by tag, suggest drilling into specific notes.

4. **Workflow chains**: Model common workflows (create -> edit -> link -> tag) and suggest the next step.

5. **`--next` or `--hint` flag**: Optionally show "what can I do next?" after any command.

6. **Footer hints**: Print a dimmed line after output: `hint: use 'hyalo show <file>' to view details, or 'hyalo edit <file>' to modify`.

## Sources

- [Command Line Interface Guidelines (clig.dev)](https://clig.dev/)
- [UX patterns for CLI tools - Lucas Costa](https://www.lucasfcosta.com/blog/ux-patterns-cli-tools)
- [Git levenshtein.c source](https://github.com/git/git/blob/master/levenshtein.c)
- [The Levenshtein distance in production](https://vishnubharathi.codes/blog/levenshtein-distance/)
- [How TheFuck works](https://nvbn.github.io/2015/10/08/how-thefuck-works/)
- [TheFuck GitHub](https://github.com/nvbn/thefuck)
- [tldr-pages GitHub](https://github.com/tldr-pages/tldr)
- [navi GitHub](https://github.com/denisidoro/navi-tldr-pages)
- [Temporal CLI discoverability proposal](https://github.com/temporalio/proposals/blob/master/cli/000-cli-improve-commands-discoverability.md)
- [AWS CLI Wizards documentation](https://docs.aws.amazon.com/cli/latest/userguide/cli-usage-wizard.html)
- [strsim crate](https://crates.io/crates/strsim)
- [inquire crate](https://crates.io/crates/inquire)
- [dialoguer crate](https://docs.rs/dialoguer)
- [clap crate](https://docs.rs/clap/latest/clap/)
- [Agent Skills: Progressive Disclosure as a System Design Pattern](https://www.newsletter.swirlai.com/p/agent-skills-progressive-disclosure)
- [hint CLI tool](https://github.com/agarthetiger/hint)
- [Comparison of Rust CLI Prompts](https://fadeevab.com/comparison-of-rust-cli-prompts/)
- [HN discussion on CLI discoverability](https://news.ycombinator.com/item?id=23329723)
