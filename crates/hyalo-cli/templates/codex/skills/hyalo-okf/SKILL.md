---
name: hyalo-okf
description: Maintain Open Knowledge Format vaults with Hyalo's OKF index, log, and conformance commands.
---

<!-- hyalo:managed -->

# Open Knowledge Format

Use `hyalo config` to locate the vault and confirm the `okf` profile.
For a read-only check, use `hyalo lint --profile okf --format text` and
`hyalo okf index --dry-run`. Inspect `hyalo okf --help` and the relevant subcommand
help before generating indexes or logs. Dry-run output is a proposal, not a completed edit.

Preserve reserved index/log files and the vault's schemas. Use Hyalo's OKF generators
for their managed sections and normal editing tools for prose outside them.
Apply generated changes only when requested, then check conformance and index drift again.
