---
paths:
  - "hyalo-knowledgebase/**"
---
Prefer `hyalo` CLI for operations on files in this directory:
- **Search/filter**: `hyalo find --property status=planned --tag iteration`
- **Body search**: `hyalo find "broken links"`
- **Title regex**: `hyalo find --property 'title~=link'`
- **Inspect config**: `hyalo config` — shows effective dir, config path, hints, format, site_prefix
- **Read frontmatter/metadata**: `hyalo find --file <path>`, `hyalo properties`, `hyalo tags`
- **Read content/sections**: `hyalo read <path>` or `hyalo read <path> --section "Heading"`
- **Mutate frontmatter**: `hyalo set`, `hyalo remove`, `hyalo append`
- **Auto-link**: `hyalo links auto --first-only --exclude-target-glob 'templates/*' --apply`
- **Move/rename (single file)**: `hyalo mv old.md --to new.md` (rewrites links across the vault)
- **Move/rename (batch)**: `hyalo mv --glob 'iterations/*.md' --property status=completed --to iterations/done/` (dry-run by default; add `--apply` to commit; builds link graph once for all files; use `--on-conflict=skip` to skip collisions)
- **Lint markdown + frontmatter**: `hyalo lint`, `hyalo lint --strict` (promotes missing-type and undeclared-property warnings to errors), `hyalo lint --rule HYALO001 --detailed`, `hyalo lint --fix --dry-run`, `hyalo lint --fix`
- **Manage lint rules**: `hyalo lint-rules list`, `hyalo lint-rules show <ID>`, `hyalo lint-rules set <ID> --enabled false`, `hyalo lint-rules set <ID> --severity warn`

Fall back to Edit for body prose changes, Write for new files, and Read when
hyalo doesn't cover the operation (e.g., reading raw markdown for rewriting).

Output format auto-detects (text on terminals, json when piped); pass `--format text`
or `--format json` to override. Run `hyalo <command> --help` if unsure.
