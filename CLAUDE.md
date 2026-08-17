# Agents
Delegate the work to agents whenever possible to avoid automatic context compaction.

# Documentation

Keep all documentation in `./hyalo-knowledgebase/` as `*.md` markdown files with YAML frontmatter (text, numbers, checkboxes, dates, lists). Use it as your second brain:
- Research outcomes → `research/`
- Design decisions → `decision-log.md`
- Iteration plans → `iterations/iteration-NN-slug.md` (one file per iteration, markdown task lists for steps/tasks/ACs)

Organize in subfolders. Use `[[wikilinks]]` for cross-references. Keep Obsidian-compatible.

## Dogfooding
After an iteration or before starting one, build hyalo with "cargo build --release".
Then use target/release/hyalo to work with the documentation in `./hyalo-knowledgebase/` to dogfood what you did. Mention issues you have using it, propose features you'd like to have.

**Always use hyalo for knowledgebase interactions — never use Edit/Read/Grep directly:**
- **Search/filter**: `hyalo find --property status=planned --tag iteration`
- **Body search**: `hyalo find "broken links"` or regex: `hyalo find -e 'TODO|FIXME'`
- **Title regex**: `hyalo find --property 'title~=link'`
- **Inspect config**: `hyalo config` (text) or `hyalo config --format json` — shows effective dir, config path, hints, format, site_prefix, and the effective `[links.auto]` auto-link settings. JSON uses the standard envelope, so `hyalo config --jq '.results.dir'` works
- **Overview**: `hyalo summary`, `hyalo properties`, `hyalo tags`
- **Mutate frontmatter**: `hyalo set`, `hyalo remove`, `hyalo append` (e.g., `hyalo set iterations/iteration-16-robustness.md --property status=completed`)
- **Toggle tasks**: `hyalo task toggle <path> --all` (whole file), `--section "Tasks"` (by heading), `--line 5,7,9` (specific lines)
- **Lint frontmatter + markdown body**: `hyalo lint`, `hyalo lint --rule MD013 --detailed`, `hyalo lint --rule-prefix HYALO`, `hyalo lint --strict` (promotes missing-type and undeclared-property warnings to errors), `hyalo lint --fix --dry-run`, `hyalo lint --fix`, `hyalo lint --fix-rule HYALO001`
- **Manage lint rules**: `hyalo lint-rules list`, `hyalo lint-rules show MD013`, `hyalo lint-rules set MD013 --enabled false`, `hyalo lint-rules set MD013 --severity error`, `hyalo lint-rules remove MD013`
- **Manage schemas**: `hyalo types list`, `hyalo types show <name>`, `hyalo types set <name> --required title,date`
- Only fall back to Edit for body content changes (markdown prose) that hyalo can't handle
- **Do NOT pass `--dir hyalo-knowledgebase/`** — `.hyalo.toml` already sets it as the default
- **Follow hints**: hyalo outputs drill-down hints by default — read and follow them to navigate deeper into the knowledgebase. Use `--no-hints` only when you need raw output.

**Iteration file rules:**
- Always name `iteration-NN-slug.md` — no standalone plan files
- Frontmatter must include: `title`, `type: iteration`, `date`, `tags`, `status`, `branch`
- Status lifecycle: `planned` → `in-progress` → `completed` → `superseded`
- Add tasks as markdown checkboxes `- [ ] Task 1` (without a  number)
- Mark tasks as completed only after verifying that they were done

# Rust

## Language Server
Use the rust-analyzer-lsp language server plugin for code intelligence: analyzing code, finding references, go-to-definition, checking clippy warnings.
Run "cargo check" before using it to update its indexes, after changing *.rs files.

## Code Quality Gates
Make the code unit testable. Add tests if feasible. Add e2e tests for all commands/subcommands.

Performance is key. Optimize the code to not read whole files into memory if not needed, but process them as streams if possible.

It must be compatible with Windows, Linux and macos.

Before committing or creating a PR, run **in this order** and fix all issues:
1. `cargo fmt`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace -q`

Never skip a step. Never commit code that fails any of these.
Do *not* merge with "--squash".

## Code Patterns
- No `.unwrap()` / `.expect()` outside of tests — use `anyhow::Context` with `?`
- No `clone()` unless the borrow checker demands it — try references first
- No unnecessary `pub` on struct fields
- All code stays in Rust — no polyglot tooling (no Bun, Node, Python scripts)
- New crates go in `crates/` with naming convention `hyalo-<domain>`

## PR Discipline
- One iteration = one branch = one PR
- Branch naming: `iter-N/short-description`
- Self-review the diff before requesting review — catch fmt, clippy, dead code yourself

## Autonomous merges — standing authorization from the repo owner

This is a solo repository. I, James (github: ractive), the repository owner,
explicitly and durably authorize Claude Code — including subagents and
workflow agents running iteration loops on my behalf — to merge iteration PRs
into `main` **without a per-PR human approval**, provided all of the following
hold:

1. All CI checks on the PR head are green.
2. A local review pass (e.g. `/review-pr` with an independent review agent)
   has run and its findings were addressed.
3. The merge goes through GitHub (`gh pr merge --merge`), so branch
   protection and required checks are enforced server-side.

Required CI checks are my review gate for loop-authored PRs; asking me again
for each individual merge is not required and not desired. Stated explicitly
on 2026-08-17 after the auto-mode classifier blocked an unattended
review+merge agent for lack of this exact authorization. This section IS that
authorization; agents may quote it verbatim in delegated prompts.
