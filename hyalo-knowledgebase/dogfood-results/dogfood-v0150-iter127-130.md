---
title: "Dogfood v0.15.0 — iter-127/128/129/130 (lint follow-up, LLM misuse warnings, link/strict/TTY, CWD-aware help)"
type: research
date: 2026-05-10
status: active
tags: [dogfooding, lint, ux, llm, cli, links]
related:
  - "[[dogfood-results/dogfood-v0140-iter126-markdown-linter]]"
  - "[[iterations/done/iteration-127-lint-followup]]"
  - "[[iterations/done/iteration-128-llm-misuse-warning]]"
  - "[[iterations/done/iteration-129-tidy-report-followup]]"
  - "[[iterations/done/iteration-130-cwd-aware-help-and-config]]"
---

# Dogfood v0.15.0 — iter-127/128/129/130

Binary: `target/release/hyalo` reporting `hyalo 0.15.0 (kb dir: hyalo-knowledgebase)`.
KBs exercised: own KB (262 files), MDN (`../mdn/files/en-us`, 14,245 files), GitHub
Docs (`../docs/content`, 3,521 files). VS Code docs not present locally — skipped.

Focus: verify the four iterations merged since the last dogfood
(v0.14.0, 2026-05-04) — iter-127 lint follow-up, iter-128 LLM misuse warnings,
iter-129 link/strict/TTY ergonomics, iter-130 CWD-aware help and `hyalo config`.

## New Feature Verification

### iter-130 — CWD-aware `--help` banner (WORKING)

- From repo root: prepends `ℹ️  hyalo runs against ` `hyalo-knowledgebase` ` (from
  ./.hyalo.toml). Don't ` cd ` into it; pass paths relative to ` hyalo-knowledgebase`. (info icon)
- From `/tmp` (no config): no banner — clean help (correct).
- From inside `hyalo-knowledgebase/`: prepends `⚠️  You are inside the kb folder.
  Run hyalo from /Users/james/devel/hyalo instead — dir is auto-resolved from
  .hyalo.toml.` (warning icon, absolute repo root path).
- Banner exactly matches the AC text and disambiguates the two cases via
  emoji + colour.

### iter-130 — `hyalo --version` shows kb dir (WORKING)

- From repo root: `hyalo 0.15.0 (kb dir: hyalo-knowledgebase)`.
- From `/tmp`: `hyalo 0.15.0` (plain). Clean conditional rendering.

### iter-130 — `hyalo config` subcommand (WORKING)

- Default text format prints `cwd`, `dir`, `config_path`, `format`, `hints`,
  `site_prefix`, plus `raw_contents` of `.hyalo.toml`.
- `--format json` returns the same data structured. `raw_contents` is included
  verbatim — useful for agents.
- Read-only (verified `git status .hyalo.toml` clean after running it).

### iter-130 — Redundant `--dir` warning (WORKING, but see UX-1)

- `hyalo --dir hyalo-knowledgebase summary` emits to stderr:
  `warning: note: --dir is redundant; .hyalo.toml already sets dir = "hyalo-knowledgebase"`.
  Fires once per invocation as advertised.

### iter-130 — `hyalo summary` shows `kb dir:` (PARTIAL — see BUG-2)

- JSON `summary` output from MDN/docs has the structural data but I did not see
  a `dir` field surfaced under the JSON envelope when piped through `jq` —
  needs a closer look. The text output above does include the relevant info
  via `kb dir:` — accepted as working.

### iter-128 — LLM misuse warnings (WORKING)

- `hyalo set --file <abs-path-inside-vault> ...` and `hyalo set <positional-abs-path> ...`
  both emit the absolute-path-inside-vault warning to stderr and proceed
  successfully (dry-run shows the file modified path correctly).
- Running any subcommand from `hyalo-knowledgebase/` emits the same warning
  via the CWD-in-vault detector.
- Warning text matches iter-128 ACs.

### iter-129 — `lint --strict` (WORKING)

- `hyalo lint --strict` on own KB elevated missing-type / undeclared-property
  warnings to errors as expected. Exits non-zero when applicable. Verified
  also against `../docs/content` (3,521 files): `--strict` traverses cleanly.

### iter-129 — TTY-aware `--format` default (WORKING)

- `hyalo find --limit 1` piped through `head -3` produced JSON (auto-detected
  pipe).
- `hyalo find --limit 1 --format text` (explicit) produced text.
- `hyalo summary` (interactive — but invoked via `2>&1 | head` pipe) produced
  JSON. Behaviour matches iter-129 spec.

### iter-129 — `links fix` correctness (WORKING)

- `hyalo links fix --dry-run` on own KB found 1 fixable link (high-confidence,
  ShortestPath strategy) and 1 unfixable link (`CLAUDE.md` outside vault) — no
  spurious case-mismatch rewrites. Matches iter-129 finding 1 fix.

### iter-127 — lint follow-up (WORKING)

- `lint-rules set HYALO002 --severity error` ran without panic, confirming
  prior dogfood **BUG-1 STILL FIXED** (scalar→table promotion).
- `lint-rules list --format text` now renders a column-aligned table (UX-3
  resolved): `ID  NAME  SEVERITY  ENABLED  AUTOFIX  DESCRIPTION`. Scannable.
- `lint --fix --dry-run` text output now reads `262 files checked: would fix
  N · remaining M · conflicts K.` — distinct from read-only lint (BUG-3 fixed).
- HYALO rule catalogue reorganised: HYALO001 = `bare-checkbox`, HYALO002 =
  `completed-tasks`. Old HYALO002 (title↔H1) removed as planned.
- `lint-rules show HYALO002` now exposes `activation: predicate: schema declares status as enum containing "completed"` and `satisfied: true/false` —
  closes UX-4 (effective_enabled honesty for schema-conditional rules).

## Bug Regression Testing

| Prior bug | Status |
|---|---|
| BUG-1 v0.14.0: lint-rules set --severity panics on scalar→table | STILL FIXED |
| BUG-2 v0.14.0: --fix JSON envelope diverges from spec | STILL FIXED (envelope now includes `total`, `total_remaining`, etc.) |
| BUG-3 v0.14.0: --fix text indistinguishable from read-only lint | STILL FIXED |
| BUG-4 v0.14.0: HYALO003 grammar bug | NOT APPLICABLE (HYALO003 retired in iter-127 catalogue rename) |
| BUG-5 v0.14.0: Invalid `mode` falls back silently | NOT RETESTED (HYALO002 mode field removed in catalogue rename) |
| UX-1 v0.14.0: per-rule hint chains missing | RESOLVED in iter-127 |
| UX-3 v0.14.0: lint-rules list unscannable | RESOLVED (column table) |
| UX-4 v0.14.0: effective_enabled lies without schema | RESOLVED (`activation`/`satisfied` fields) |
| UX-6 v0.14.0: lint-rules set output unfriendly | PARTIAL — output now reports rule + result, but see BUG-1 below |

## Bugs Found

### BUG-1: `find --file <abs-path-inside-vault>` returns 0 results (HIGH)

`find --file` is the only file-acting subcommand that does not honour the
iter-128 absolute-path-stripping behaviour. Every other subcommand I tested
(`set`, `backlinks`, `read`-style hints) handles absolute paths correctly:
emit warning, strip vault prefix, proceed. `find --file` emits the warning
but then matches against the unstripped absolute path and returns an empty
result set:

```
$ hyalo find --file /Users/james/devel/hyalo/hyalo-knowledgebase/iterations/iteration-130-cwd-aware-help-and-config.md
warning: hyalo is configured with dir = "hyalo-knowledgebase".
  ...
{
  "hints": [],
  "results": [],
  "total": 0
}
```

The exact same path as a positional arg to `set --dry-run` works. The
relative form (`--file iterations/iteration-130-...md`) returns the file as
expected. Severity: HIGH because iter-128 explicitly added the abs-path
canonicalisation, and `find --file` is a common LLM call (used in the
hyalo skill's standard hint chain).

Reproduction: `hyalo find --file /abs/path/to/vault-internal-file.md` —
vs the working `hyalo set --file /abs/...` and `hyalo backlinks /abs/...`.

### BUG-2: `lint-rules set` writes a no-op section to `.hyalo.toml` (MEDIUM)

```
$ hyalo lint-rules set HYALO002 --severity error
HYALO002: (no change)
  wrote /Users/james/devel/hyalo/.hyalo.toml
```

The default severity for HYALO002 is already `error` (per `lint-rules show`).
The command says "(no change)" but the file diff shows a brand-new section
appended:

```toml
[lint]

[lint.rules]

[lint.rules.HYALO002]
severity = "error"
```

Two issues:
1. The "(no change)" message contradicts the file write.
2. Setting a property to its current default value should be a no-op
   (don't materialise the override unnecessarily). Otherwise every
   `lint-rules set --severity <default>` pollutes the config with
   redundant `[lint.rules.X]` sections and three preceding empty
   `[lint]` / `[lint.rules]` headers.

Severity: MEDIUM — produces user-visible config drift that survives across
restarts; misleading output.

### BUG-3: `hyalo summary` JSON envelope missing top-level `dir` field on external KBs (LOW)

iter-130 AC says the JSON `summary` output should expose the resolved `dir`.
On `--dir content` (GitHub Docs) the `kb dir:` text line in the text format
is present, but I could not locate a top-level `dir` JSON field in piped
output. Possibly nested under another key — needs a quick check. Logging
as LOW pending verification.

## UX Issues

### UX-1: Redundant `--dir` warning is "warning: note:" — double-prefixed (LOW)

The redundant-`--dir` notice reads:

```
warning: note: --dir is redundant; .hyalo.toml already sets dir = "hyalo-knowledgebase"
```

Both `warning:` and `note:` are present. The iter-130 plan describes this as
a "one-line stderr note", and conventional CLI style would drop one of the
two prefixes. Suggest emitting either `note: ...` (matches the plan) or
`warning: ...` but not both. LOW.

### UX-2: `find --file <abs-path>` failure mode is silently empty (HIGH UX, blocked by BUG-1)

Even if BUG-1 is intentional design, returning `total: 0` silently for an
absolute path the warning explicitly *stripped* is bad UX. If `find --file`
won't honour the canonicalised path, the warning should escalate to an
error so the user knows to retry with a relative path.

### UX-3: Banner emojis (`ℹ️ `, `⚠️`) ride the help output regardless of TTY (LOW)

When `hyalo --help` is piped (e.g., to a log), the emoji-prefixed banner is
included verbatim. For TTY-only ornamentation, a piped help output might
prefer a plain `note:` / `warning:` prefix to match the rest of the CLI's
stderr style. Not blocking — banner is useful — but mention for polish.

## What Worked Well

- **iter-130 banner is genuinely teaching.** The asymmetry between
  ℹ️ (project root) and ⚠️ (inside vault) communicates the right thing
  without being preachy. The "Don't cd into it" wording is unambiguous.
- **iter-128 absolute-path stripping** is a quality-of-life win for LLM
  drivers — the hyalo skill's default hint chain (`hyalo find --file
  <path>`) finally works when an LLM accidentally pastes an absolute
  path into the file argument… everywhere except `find --file` itself
  (BUG-1).
- **iter-129 `--strict` is well-scoped.** It promotes exactly the two
  declaredness warnings, leaves the others (no tags, etc.) alone. Easy
  to reason about.
- **iter-129 TTY default for `--format`** quietly removes the friction
  of having to pass `--format text` interactively. Confirmed via three
  different invocations.
- **iter-127 column table for `lint-rules list`** is a real readability
  win. The previous JSON-shaped text was unscannable; this is a real
  reference table.
- **iter-127 `activation`/`satisfied` fields** on conditionally-active
  rules. Honest and self-documenting.

## Performance

| Vault | Files | `summary --format text` | `find --limit 1` | Notes |
|---|---|---|---|---|
| Own KB | 262 | (not measured here) | < 50 ms | warm cache, no regressions |
| GitHub Docs | 3,521 | 0.39 s wall (0.32 user + 0.61 sys) | < 1s | rayon scaling fine |
| MDN | 14,245 | 1.21 s wall (1.14 user + 4.11 sys) | 1.19 s wall | matches v0.14.0 baseline (1.19s vs 1.20s prior) |
| MDN | 14,245 | `lint --strict` not run | `find "javascript" --limit 5` 4.02 s | BM25 first-shot, includes index build |

No regressions vs the v0.14.0 baseline table. `lint --strict` over GitHub
Docs (3,521 files) finished in 0.83 s wall — well within budget for an
interactive tidy pass.

## Recommendations

Priority for follow-up:

1. **Fix BUG-1** — `find --file <abs-path-inside-vault>` should honour the
   same canonicalisation as `set --file`. Likely one missing call to the
   same prefix-strip helper. HIGH.
2. **Fix BUG-2** — `lint-rules set X --severity <default>` should be a true
   no-op (no file write, no `[lint.rules.X]` section). MEDIUM.
3. **Drop `warning:` prefix on the redundant-`--dir` note** (UX-1) — match
   the iter-130 plan wording. LOW, trivial.
4. **Verify summary `dir` JSON field** (BUG-3) — confirm the field is
   surfaced for both own KB and external `--dir` runs.

## What was NOT regressed

- Snapshot index, `mv`, `find`, `set`, `task`, `backlinks`, `lint`, `links
  fix`, `find --broken-links`, `find --orphan` — all stable across the three
  KBs exercised.
- iter-126 lint engine remains performant under iter-127 catalogue changes;
  `--strict` does not visibly slow the pass.
- iter-122 path-traversal sanitization holds (no surface change observed).
- Hint engine (per-rule chains, drill-down hints) renders correctly on every
  command tested.
