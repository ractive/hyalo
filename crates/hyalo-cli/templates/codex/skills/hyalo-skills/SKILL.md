---
name: hyalo-skills
description: Validate and maintain Agent Skills collections using Hyalo's skills profile, including Codex and Claude skill directories.
---

<!-- hyalo:managed -->

# Agent Skills collections

Use `hyalo lint --profile skills --format text` from the collection root.
The profile binds `**/SKILL.md` to its skill schema and includes hidden
`.agents/skills/**` and `.claude/skills/**` subtrees under the selected vault.
A project whose configured vault is a separate notes directory requires an explicit
`--dir .` to validate project-root skills. Do not rewrite its configuration to run a check.

Required frontmatter is `name` and `description`; the name must match the directory.
Use `hyalo types show skill` on an initialized skills vault and command help for
schema details. Keep descriptions focused on actual triggers. Preserve host-specific
metadata where supported; Codex appearance and invocation policy live in
`agents/openai.yaml`, which markdown lint does not validate.

Make requested edits, then rerun conformance on the affected files. A clean lint result
checks structure, not whether a skill follows user intent or makes good decisions.
