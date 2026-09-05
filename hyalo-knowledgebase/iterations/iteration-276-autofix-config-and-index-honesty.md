---
title: "Iteration 276 — Autofix, config and index honesty: disable-next-line, list-indented fences, recipe safety, schema typos, snapshot version, CWD paths"
type: iteration
date: 2026-09-05
tags: [iteration, lint, schema, index, config, mutations, dogfooding]
status: completed
branch: iter-276/autofix-config-index-honesty
priority: 2
related:
  - "[[dogfood-results/dogfood-v0220-post-batch-271-274]]"
  - "[[iterations/iteration-271-write-and-rewrite-safety]]"
  - "[[iterations/iteration-273-index-and-named-file-honesty]]"
  - "[[iterations/iteration-274-hints-help-and-contract-polish]]"
  - "[[decision-log]]"
---

# Iteration 276 — Autofix, config and index honesty

## Goal

Close the two remaining autofix corruption bugs, the shipped-recipe incident, and every config,
index, schema and write-side contract gap from
[[dogfood-results/dogfood-v0220-post-batch-271-274]] (BUG-4, 5, 6, 11, 12, 19, 20, 21, 22, 27,
28, 29, 30, 33, 34, 35, 38, 40, 41, 42, 43, 44; UX-1, 3, 4, 5, 7, 14; G4, G5). Shaped like
iteration 274: grouped by the code it touches, most items one-function fixes with a fixture in
the report.

Rules: **no new CLI flags**; every fixture from the report becomes a test; WIP commit after each
group; time-box — anything still open when the gates are green goes to `backlog/` with the
report's repro.

## LINT — autofix safety (BUG-4, 5, 19, 43; UX-7)

- [x] LINT-1 (BUG-4): `markdownlint-disable-next-line` suppresses the **following** line
      only, for id and alias forms, standalone and trailing; the line carrying a trailing
      `-next-line` comment is not itself suppressed. Fixture `nl.md` from the report verbatim;
      `lint --fix` must leave lines 8, 13 and 16 byte-identical and rewrite line 15.
- [x] LINT-2 (BUG-5): `BodySpans` recognises fenced code blocks indented inside list items
      (CommonMark: fence indented up to the list item's content column), for ```` ``` ```` and
      `~~~`, nested lists included, so every exempted rule (MD009/011/012/019/022/023/034/042)
      stays silent inside them. Fixture: the GitHub Docs `removing-dependabot-access-to-public-
      registries.md` shape (4-space fence under `1.`), plus a 2-space `-` item and a
      blockquoted list. Re-run `lint --fix --dry-run` on a GitHub Docs copy and record that
      the MD034 proposal at line 224 is gone.
- [x] LINT-3 (BUG-43): an unknown rule id or alias in a suppression comment produces a
      one-line `-q`-proof warning naming the comment's line (markdownlint reports these);
      known ids inside a fenced sample stay ignored.
- [x] LINT-4 (BUG-19): `--max-per-rule 0` means unlimited, as `lint --help` says and
      `--limit 0` already does.
- [x] LINT-5 (UX-7): decide-or-implement a per-rule option surface for MD010's `code_blocks`
      (markdownlint has it; 215 tab fixes inside Go samples on GitHub Docs where gofmt mandates
      tabs). Preferred: `lint-rules set MD010 --option code_blocks=false` if `lint-rules set`
      already takes an option map; otherwise a DEC saying `<!-- markdownlint-disable
      no-hard-tabs -->` is the answer and why.

## RECIPES — shipped documentation must never be a paste-able write (BUG-6; UX-4, 5)

- [x] RECIPE-1 (BUG-6): every mutating `--jq` example in `templates/skill-hyalo.md`,
      `templates/rule-knowledgebase.md`, `pi-package/` and `.claude/CLAUDE.md` carries
      `--dry-run` in the shipped text (the `skipped_count` example at `skill-hyalo.md:235`
      first). `check-jq-recipes` **fails** on a mutating recipe that lacks `--dry-run` instead
      of appending it, the way it already fails on `--apply`; the gate's header comment says
      so.
- [x] RECIPE-2 (UX-4): the `hints` recipe at `skill-hyalo.md:251` cannot work because `--jq`
      strips hints; either keep `hints` in the envelope under `--jq` (DEC) or replace the
      example with one that runs against the plain JSON. Document whichever in `--jq`'s help.
- [x] RECIPE-3 (UX-5): the broken-links recipe in `.claude/CLAUDE.md` and the templates prints
      `#\(.fragment // "")` after the target so anchor-only breaks are readable.

## SCHEMA — typos must not validate nothing (BUG-20, 42; G5)

- [x] SCHEMA-1 (BUG-20): `[schema]`, `[schema.types.<t>]` and the wrong nesting
      `[schema.<t>]` reject unknown keys with the same "unknown field `x`, expected one of …"
      error `[scan]` gives; `hyalo config` reports `malformed: true` with the diagnostic;
      `lint`, `find --strict`, `set --validate` refuse per DEC-290. Fixture: `requried`.
- [x] SCHEMA-2 (BUG-42): `required = ["title"]` means present and non-empty, not `string`;
      `type = "string"` is the explicit opt-in. `title: 2024` passes `lint` and
      `set --validate title=2024` under a required-only schema. Amend the schema DEC that
      introduced the implicit string constraint (name it in the Outcome) and update
      `types --help`.
- [x] SCHEMA-3 (G5, BUG-41): `set --help` documents the scalar coercion table (`true`,
      `1e3` → `1000.0`, dates, `null`/`~` → strings, YAML-1.1 keys quoted) in one block; if a
      null-writing form exists already (`K=` was rejected by 274 for filters, not for `set`),
      say so; otherwise record the gap as a DEC won't-do or a backlog file, no flag.

## INDEX — named index files and snapshot versions (BUG-11, 12; G4)

- [x] INDEX-1 (BUG-11): `--index-file <unreadable>` is an exit-1 JSON envelope like
      `--files-from /nope`, never a silent disk scan; a **missing in-vault** `.hyalo-index`
      under bare `--index` keeps today's fallback-with-warning, and that warning is
      `-q`-proof.
- [x] INDEX-2 (BUG-12, G4): the snapshot header carries a format version; a snapshot whose
      version predates the binary's is refused with a one-line warning naming the version
      pair and falls back to disk (same shape as the site-prefix mismatch refusal);
      `summary --index` and `hyalo config` expose the version so an agent can tell an old
      index from a fresh one. Bump the version for the 272 SelfAnchor and 273 header changes
      so the Sep 3 MDN index is refused. Fixture: an index written with the previous version
      constant.
- [x] INDEX-3 (BUG-30): DEC-302's text says the blind spot is up to ~2 s (whole-second
      mtimes plus a one-second tolerance), and `create-index --help` says the same.

## PATHS — CWD inside the vault and the vault-root config (BUG-21, 22, 28)

- [x] PATH-1 (BUG-21): when the CWD is inside the configured vault and `<cwd>/<path>` exists
      and is a different file from `<vault>/<path>`, every path-taking command prints a
      `-q`-proof warning naming both candidates and the one it used; `mv --to ../deep/`
      reports "path contains `..`" like the source check. No CWD-relative resolution and no
      flag (DEC-304 stays).
- [x] PATH-2 (BUG-22): `[links] case_insensitive = "false"` returns the canonical on-disk
      path on a case-folding filesystem or the DEC says exact match means exact bytes and
      `config` warns that `false` on macOS/Windows is weaker than `auto`. Correct the 274
      Outcome line either way.
- [x] PATH-3 (BUG-28): the `--dir is redundant` note fires only when `.hyalo.toml` actually
      sets `dir`, and says "the default is `.`" otherwise (`run.rs:1320`).

## WRITE — the last non-addressed bytes and envelope leaks (BUG-33, 34, 35, 38, 40; UX-1, 14)

- [x] WRITE-1 (BUG-33): `set`/`append`/`remove` leave a closing fence with trailing
      whitespace byte-identical; the inserted line goes above it.
- [x] WRITE-2 (BUG-34): decide the opener: either `--- ` (trailing whitespace) opens
      frontmatter as DEC-293's wording claims, or DEC-293 is corrected to "opener must be
      exactly `---`" and HYALO005 names the shape ("line 1 looks like an opener with trailing
      whitespace") instead of MD009 (UX-14). Either way `set` never prepends a second block
      above an existing one.
- [x] WRITE-3 (BUG-35): `set`/`append`/`remove`/`properties rename` on an unparsable file put
      the YAML diagnostic in the envelope's `cause` and print no bare `error:` line under
      `--format json`.
- [x] WRITE-4 (BUG-38): `tags rename` keeps a flow-style `tags: [..]` in flow style, using the
      same emitter as `set --tag`.
- [x] WRITE-5 (BUG-40): `1. [ ]` ordered-list tasks and `-  [ ]` (two spaces) are tasks for
      `--fields tasks`, `summary` and `task toggle`; `- [ ]no space` stays as it is.
- [x] WRITE-6 (UX-1): bulk `set`/`append`/`remove` envelopes carry `skip_reason` per skipped
      file (`unchanged`, `unparsable`, `schema`, `list_collapsed`), so a same-value write is
      distinguishable from a refusal.
- [x] WRITE-7 (BUG-44): the `summary` near-duplicate-value warning requires a real string
      similarity (shared prefix or edit distance) and a minimum value count before it speaks.

## HELP — one-line corrections (BUG-27, 29; UX-3)

- [x] HELP-1 (BUG-27): `lint-rules show` prints no `=> lint-rules set` hint for a
      non-configurable rule (SCHEMA).
- [x] HELP-2 (BUG-29): `hyalo --help` line 35 / 648 and `lint --help` line 179 describe exit 2
      as "usage (clap) or internal error" per DEC-307.
- [x] HELP-3 (UX-3): `mv --help` says in one sentence why `--on-conflict` offers `error` and
      `skip` but not `overwrite`.

## Shared closing tasks

- [x] Changelog entries via `hyalo changelog add` (one per group, listing the items).
- [x] DECs in [[decision-log]]: snapshot format version (INDEX-2), required-vs-string
      (SCHEMA-2), the opener decision (WRITE-2), MD010 option or won't-do (LINT-5); DEC-293
      and DEC-302 amended in place with a dated line.
- [x] Help texts, `rule-knowledgebase.md`, `skill-hyalo.md`, `.claude/CLAUDE.md` and the
      README's CI section (if the gate change is user-visible) updated in the same PR.
- [x] Every unfinished item moved to `backlog/` with its repro.
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, every xtask `check-*`
      gate including the stricter `check-jq-recipes`.

## Acceptance criteria

- [x] `nl.md` and the list-indented-fence fixtures are byte-identical under `lint --fix`
      except for the lines that must change; the GitHub Docs line-224 proposal is gone.
- [x] `check-jq-recipes` fails on a mutating recipe without `--dry-run`, and the shipped
      documents pass it; pasting any shipped recipe verbatim writes nothing.
- [x] `requried = [...]` is a malformed config; `title: 2024` passes a required-only schema.
- [x] `--index-file /nope` exits 1 with an envelope; the Sep 3 MDN index is refused with the
      version warning; `hyalo config` shows the snapshot version.
- [x] From `kb/sub/` with `kb/a.md` and `kb/sub/a.md`, `set a.md` warns and names both files.
- [x] `set` on a file closing with `--- ` changes only the inserted line; JSON-mode errors on
      unparsable files are a single envelope with `cause`.
- [x] Gates green; changelog; DECs.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-271-274]] — BUG-4, 5, 6, 11, 12, 19, 20, 21, 22, 27, 28, 29, 30, 33, 34, 35, 38, 40, 41, 42, 43, 44; UX-1, 3, 4, 5, 7, 14; G4, G5
- [[iterations/iteration-271-write-and-rewrite-safety]] — DEC-293, DEC-294, `BodySpans`
- [[iterations/iteration-273-index-and-named-file-honesty]] — DEC-301, DEC-302, snapshot header
- [[iterations/iteration-274-hints-help-and-contract-polish]] — `check-jq-recipes`, DEC-307
- [[decision-log]] — DEC-290, DEC-293, DEC-294, DEC-302, DEC-304, DEC-307
