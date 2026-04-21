---
paths:
  - "hyalo-knowledgebase/**"
---
Prefer `hyalo` CLI for operations on files in this directory:
- **Search/filter**: `hyalo find --property status=planned --tag iteration --format text`
- **Body search**: `hyalo find "broken links" --format text`
- **Title regex**: `hyalo find --property 'title~=link' --format text`
- **Read frontmatter/metadata**: `hyalo find --file <path>`, `hyalo properties`, `hyalo tags`
- **Read content/sections**: `hyalo read <path>` or `hyalo read <path> --section "Heading"`
- **Mutate frontmatter**: `hyalo set`, `hyalo remove`, `hyalo append`
- **Auto-link**: `hyalo links auto --first-only --exclude-target-glob 'templates/*' --apply`
- **Move/rename**: `hyalo mv` (rewrites links across the vault)

Fall back to Edit for body prose changes, Write for new files, and Read when
hyalo doesn't cover the operation (e.g., reading raw markdown for rewriting).

Use `--format text` for compact output. Run `hyalo <command> --help` if unsure.
