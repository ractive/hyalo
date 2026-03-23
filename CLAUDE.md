# Agents
Delegate the work to agents whenever possible

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
- **Read**: `hyalo find`, `hyalo summary`, `hyalo properties`, `hyalo tags`
- **Mutate frontmatter**: `hyalo set`, `hyalo remove`, `hyalo append` (e.g., `hyalo set --property status=completed --file iterations/iteration-16-robustness.md`)
- Only fall back to Edit for body content changes (markdown prose, task checkboxes) that hyalo can't handle
- **Do NOT pass `--dir hyalo-knowledgebase/`** — `.hyalo.toml` already sets it as the default

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
3. `cargo test --workspace`

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
