---
name: hyalo-madr
description: Maintain Markdown Architecture Decision Records with Hyalo's MADR profile and table-of-contents commands.
---

<!-- hyalo:managed -->

# Architecture decision records

Inspect `hyalo config`, `hyalo types show adr`, and `hyalo madr --help`.
Use `hyalo lint --profile madr --format text` for conformance and
`hyalo madr toc --dry-run` to inspect table-of-contents drift.

Keep numbering, status transitions, supersession links, and the configured ADR path
consistent with the existing collection. Use the relevant command's help before creating
or superseding records. Do not infer that an accepted decision is superseded from age alone.
Apply requested metadata changes with Hyalo and prose with ordinary editing tools.
After edits, lint affected records and preview the table of contents again.
