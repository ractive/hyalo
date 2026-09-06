---
name: hyalo
description: "Search and edit structured markdown knowledgebases with the Hyalo CLI: frontmatter, tags, tasks, links, and document structure."
---

<!-- hyalo:managed -->

# Hyalo knowledgebase workflow

Use the installed `hyalo` executable. In a Hyalo source checkout, `target/release/hyalo`
is an alternative when present. If unavailable, report that and suggest the project's
documented installation (`cargo install hyalo-cli`); do not install software as a
side effect of a knowledgebase query.

Run from the project root containing `.hyalo.toml`, then use `hyalo config --format json`
to locate and confirm the vault. Commands also work inside that configured vault,
including its nested folders. Other project subdirectories (for example `src/`
beside a `notes/` vault) do not inherit the vault configuration: return to the
project root before querying or editing. Do not assume the vault is named
`hyalo-knowledgebase` and do not repeat `--dir` when configuration already selects it.
File arguments are relative to the vault, including when launched from a nested folder.

Use `hyalo summary --format text` for orientation. Then select the smallest query
that answers the request:

```sh
hyalo find "release blockers" --format text
hyalo find --property status=planned --tag iteration --format text
hyalo read iterations/example.md --section Tasks --format text
hyalo backlinks iterations/example.md --format text
hyalo set iterations/example.md --property status=in-progress
hyalo task toggle iterations/example.md --line 12
hyalo lint iterations/example.md --format text
```

The paths above are examples: resolve actual files before running mutations.
Use `hyalo <command> --help` for exact syntax, advanced filters, links, or bulk operations.
`find`'s positional query is ranked search; `-e` is regex. Quote filter expressions
and paths as single arguments using the active shell's quoting rules.

Pass `--format text` for compact reading or `--format json` for structured output.
JSON is an envelope; results live in `.results`. `--jq` operates on that envelope
and cannot be combined with text format. Respect reported truncation; narrow the query
or increase `--limit` only when needed. Read diagnostics and exit status before treating
an empty result as success. Hints marked as writes require the same scope as any mutation.

Prefer Hyalo for structured metadata changes and task toggles. Use normal editing
tools for body prose and new document bodies; preserve frontmatter and wikilinks.
Lint the changed files before handoff. This is an instruction, not an automatic hook.

For an audit, inspect and report. When repairs are requested, apply only those in scope.
Preview bulk or link repairs with the command's dry-run mechanism before applying them.
Avoid snapshot indexing for a small one-off query. For repeated work on a large vault,
`create-index` and `--index` help; rebuild after external edits and drop the index
when done. Never ignore an index staleness warning.

Profile skills describe OKF, MADR, Agent Skills, and changelog conventions. Use only
the profile relevant to the task and active configuration. Plugin and project installations
contain the same workflows; use one copy, and do not execute a task twice.
