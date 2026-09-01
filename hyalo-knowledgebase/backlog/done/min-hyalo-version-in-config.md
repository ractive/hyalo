---
title: "Vaults declare a minimum hyalo version in .hyalo.toml"
type: backlog
date: 2026-09-01
status: wont-do
origin: mapl-memory sources-provenance design discussion 2026-09-01
priority: medium
tags:
  - config
  - compatibility
  - dx
---

## Problem

A vault's conventions can depend on hyalo features that only exist from a certain
release on. Concrete case: mapl-memory wants to turn `sources:` from a list of strings
into a list of objects (`- ref: github:comparis/neon` + `commit`, `version`, `read`
…) and query them with `find --property sources.ref=…`, which needs the
sequence-descending dot-paths of 0.21.0. Anyone running an older hyalo against that
vault gets two failure modes:

- **loud**: `lint --strict` rejects the object entries against the schema — fine;
- **silent**: `find --property sources.ref=…` returns no matches, and an agent
  concludes "nobody cites this source". This is the dangerous one.

Today nothing lets a vault say "you need at least version X". The requirement is
implicit and only discovered when a query returns wrong results.

## Proposal

A top-level key in `.hyalo.toml`:

```toml
min_hyalo_version = "0.21.0"
```

Semantics:

- Every command that loads the config compares its own version with the declared
  minimum. Older → **refuse to run** with a clear message naming both versions and
  the install/upgrade command. (Not a warning: the silent-wrong-result case is exactly
  what a warning would not prevent for agents that don't read stderr.)
- Newer or equal → nothing.
- Missing key → nothing (backward compatible for every existing vault).
- Semver comparison; pre-release/build metadata of the running binary ignored.
- `hyalo lint` additionally flags a `min_hyalo_version` that is *higher than the
  running binary* the same way (it is the first command CI runs).
- Optional: `hyalo config check` prints the declared minimum and the running version.

## Why in the config and not elsewhere

The vault owns the requirement — it is a property of the vault's schema and
conventions, not of any one machine. CI can pin `setup-hyalo` and CLAUDE.md can say
"requires ≥ 0.21", but only the config is read by every code path (local, CI, agents)
on every invocation, so only the config can fail fast everywhere.

## Acceptance criteria

- [ ] `min_hyalo_version` is parsed from `.hyalo.toml` (optional, semver string; invalid value = config error)
- [ ] Any command that loads the config exits non-zero with a clear message when the running version is lower
- [ ] Missing key changes nothing; equal/higher version changes nothing
- [ ] Message names the required and running versions and the upgrade command
- [ ] Documented in the config reference and mentioned in the schema docs next to the `sources`-style examples
- [ ] Test: vault with `min_hyalo_version` above the binary → exit 1 on `find`, `lint`, `set`; at/below → normal behavior

## Closed 2026-09-01 — won't do

Dropped after review. The key cannot protect the case that motivated it: every
binary older than the release that ships the gate never understands
`min_hyalo_version`, so the mapl-memory / 0.21 dot-path skew stays unprotectable
by definition. `ConfigFile` is `deny_unknown_fields`, so a new key already makes
older binaries warn on every run and refuse writes; the only gap is schema-less
reads. That residual value pays off solely for vaults used from several
installs, which is not the situation today. Re-open if a shared vault appears;
the iteration plan lived in closed PR #303.
