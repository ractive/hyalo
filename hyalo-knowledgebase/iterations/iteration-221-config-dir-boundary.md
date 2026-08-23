---
title: "Iteration 221 — config dir boundary (untrusted .hyalo.toml write-scope escape)"
type: iteration
date: 2026-08-23
status: planned
branch: iter-221/config-dir-boundary
tags: [iteration, security, config, write-path]
related:
  - "[[reviews/adversarial-review-2026-08-23]]"
  - "[[reviews/deep-analysis-2-2026-08-23]]"
  - "[[iterations/iteration-201-config-trust]]"
---

# Iteration 221 — config dir boundary

## Goal

Close the one HIGH finding from [[reviews/adversarial-review-2026-08-23]]
(H-1, re-confirmed as F-6 in [[reviews/deep-analysis-2-2026-08-23]]): a
project-local `.hyalo.toml`'s `dir` value is honored verbatim, so an
untrusted cloned repo can point the vault root at its own parent or an
absolute path like `$HOME` and every downstream boundary gate then defends
containment against that attacker-chosen root. This is the release gate for
the security story and is a deliberate revisit of iter-201's decision.

## Context

**H-1 (HIGH, VERIFIED)** — `crates/hyalo-cli/src/config.rs:706`:

```rust
dir: cfg.dir.map(PathBuf::from).unwrap_or(defaults.dir),
```

The configured `dir` is taken with no check that it is at-or-below the
config directory; `..` and absolute paths both pass. Verified repro: a
malicious repo at `/tmp/evilrepo` with `docs/.hyalo.toml` containing
`dir = ".."` lets `hyalo mv docs/a.md stolen.md` move a file OUT of the
repo into the parent; `dir = "/Users/james"` is accepted silently
(`hyalo config` shows it, no warning).

Asymmetry the fix must resolve: the **ancestor-adoption** path already has
containment (`config.rs:426-429`, `canonical_cwd.starts_with(&vault)`), and
`[changelog] path` is validated against the config dir — but the
**local-config `dir`** path has neither gate. iter-201 (DEC-069/070/071)
deliberately moved toward "no silent config discard"; the decision to
revisit is narrow: honoring the config is fine, honoring its *scope
expansion* silently is not.

Threat model note: CLAUDE.md instructs agents to run hyalo commands
verbatim from hints, so a hostile repo + a normal agent loop is a plausible
write-scope-escape primitive — this is exactly the surface the boundary
layer (iter-202) is supposed to protect.

## Tasks

- [ ] For a **project-local** config (discovered in cwd or an ancestor, not
      `--dir`, not the global/XDG config), reject or contain a `dir` that
      resolves above the config directory or is absolute. Decide the exact
      policy (hard refuse vs. clamp-to-config-dir-with-loud-warning) and
      record it in the decision log, explicitly noting how it interacts with
      iter-201's DEC-069/070/071 "no silent discard" stance
- [ ] Preserve the legitimate cases: an explicit `--dir` (the user's own
      choice), the global config, and a `dir` that stays at-or-below the
      config directory must all continue to work unchanged
- [ ] Whatever the policy, emit a loud stderr `note:`/`warning:` when a
      config's requested vault root is not at-or-below the config dir,
      mirroring `announce_ancestor_config`'s treatment — never silent
- [ ] Audit the other config-supplied paths that feed the filesystem
      (`site_prefix` is display-only, but re-check `[okf]`, `[scan]`, index
      locations) for the same "config redefines its own boundary" shape
- [ ] Tests: e2e reproducing the H-1 `dir = ".."` and absolute-path escapes
      and asserting they are refused/contained + warned; regression tests
      that `--dir`, global config, and in-bounds relative `dir` still work;
      a test that the ancestor-adoption containment is unchanged

## Acceptance criteria

- [ ] The H-1 repro (`dir = ".."` in a cloned repo, then `hyalo mv`) no
      longer moves a file outside the config directory
- [ ] An absolute `dir` in a project-local config is refused or clamped, and
      always warned — never silently honored
- [ ] `--dir <anywhere>` (explicit user intent) and an in-bounds relative
      `dir` both behave exactly as before
- [ ] A decision-log entry records the policy and its relationship to
      iter-201

## Non-goals

- Windows drive-relative / ADS path gaps (M-2 → [[iterations/iteration-222-security-robustness-batch]])
- Sandboxing hyalo against a fully hostile repo beyond the write-scope root
  (out of scope for a local single-user CLI; document the residual model)
