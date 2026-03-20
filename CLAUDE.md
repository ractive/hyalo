# Agents
Delegate the work to agents whenever possible

# Documentation

Keep all documentation in `./hyalo-knowledgebase/` as `*.md` markdown files with YAML frontmatter (text, numbers, checkboxes, dates, lists). Use it as your second brain:
- Research outcomes → `research/`
- Design decisions → `decision-log.md`
- Iteration plans → `iterations/iteration-NN-slug.md` (one file per iteration, markdown task lists for steps/tasks/ACs)

Organize in subfolders. Use `[[wikilinks]]` for cross-references. Keep Obsidian-compatible.

**Iteration file rules:**
- Always name `iteration-NN-slug.md` — no standalone plan files
- Frontmatter must include: `title`, `type: iteration`, `date`, `tags`, `status`, `branch`
- Status lifecycle: `planned` → `in-progress` → `completed` → `superseded`
- Add tasks as markdown checkboxes `- [ ] Task 1` (without a  number)
- Mark tasks as completed only after verifying that they were done

# Rust

## Language Server
Use the rust-analyzer-lsp language server plugin for code intelligence: analyzing code, finding references, go-to-definition, checking clippy warnings.
Be aware that it takes some time for changed files to be indexed again.

## Code Quality Gates
Make the code unit testable. Add tests if feasible. Add e2e tests for all commands/subcommands.

Performance is key. Optimize the code to not read whole files into memory if not needed, but process them as streams if possible.

It must be compatible with Windows, Linux and macos.

Before committing or creating a PR, run **in this order** and fix all issues:
1. `cargo fmt`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`

Never skip a step. Never commit code that fails any of these.

## Code Patterns
- No `.unwrap()` / `.expect()` outside of tests — use `anyhow::Context` with `?`
- No `clone()` unless the borrow checker demands it — try references first
- No unnecessary `pub` on struct fields
- All code stays in Rust — no polyglot tooling (no Bun, Node, Python scripts)
- New crates go in `crates/` with naming convention `bunny-api-<domain>`

## PR Discipline
- One iteration = one branch = one PR
- Branch naming: `iter-N/short-description`
- Self-review the diff before requesting review — catch fmt, clippy, dead code yourself
