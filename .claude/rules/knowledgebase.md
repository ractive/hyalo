---
paths:
  - "hyalo-knowledgebase/**"
---
Prefer `hyalo` CLI for operations on files in this directory:
- **Search/filter**: `hyalo find` instead of Grep/Glob
- **Read frontmatter/metadata**: `hyalo find --file`, `hyalo properties`, `hyalo tags`
- **Read content/sections**: `hyalo read --file <path>` or `hyalo read --section "Heading"`
- **Mutate frontmatter**: `hyalo set`, `hyalo remove`, `hyalo append`
- **Move/rename**: `hyalo mv` (rewrites links across the vault)

Fall back to Edit for body prose changes, Write for new files, and Read when
hyalo doesn't cover the operation (e.g., reading raw markdown for rewriting).

Use `--format text` for compact output. Run `hyalo <command> --help` if unsure.
