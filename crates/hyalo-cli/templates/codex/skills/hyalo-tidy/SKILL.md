---
name: hyalo-tidy
description: Audit or tidy a Hyalo knowledgebase for broken links, orphan documents, stale content, and inconsistent metadata.
---

<!-- hyalo:managed -->

# Knowledgebase maintenance

Resolve the installed `hyalo` executable and run `hyalo config --format json`.
Use the configured vault and existing schemas. If the executable or configuration is
missing, report what is needed before doing maintenance.

An audit or health question is read-only. A request to tidy or repair authorizes relevant
changes, not deletion of notes based only on age or orphan status.

Orient with `hyalo summary --format text`, `hyalo lint --format text`, and
`hyalo find --broken-links --format text`. Inspect individual files with `hyalo read`.
Check schemas with `hyalo types list` before proposing metadata normalization.
An orphan can be a legitimate entry point; stale status requires evidence from its content.

Prioritize concrete findings. When authorized to repair, preview applicable fixes
(`hyalo lint --fix --dry-run`; consult `hyalo links --help` for link repairs).
Use Hyalo for frontmatter and tasks, normal editing tools for prose, and preserve meaning.
Do not apply uncertain fuzzy link replacements without inspecting the candidates.

Run lint on changed files and recheck the original findings. Report what changed,
what remains unresolved, and any skipped or failed checks. Keep audit-only runs free
of writes, including creating or dropping snapshot indexes.
