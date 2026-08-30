---
title: "Iteration 254 — dogfood v0.22.0 fixes: help fragments, stale shape docs, tidy recipe"
type: iteration
date: 2026-08-30
tags: [iteration, dogfood-fixes, help-text, find-shape]
status: completed
branch: iter-254/dogfood-v0220-fixes
depends-on: "[[iterations/iteration-252-find-result-shape]]"
---

# Iteration 254 — dogfood v0.22.0 fixes: help fragments, stale shape docs, tidy recipe

## Goal

Close the findings of [[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]]
that make the 0.22.0 surface misleading to an agent: one silently-wrong
bundled recipe, sixteen sentence-fragment one-liners left by the iter-251
split, help text that still describes the pre-252 result shape, and a
handful of help-vs-behaviour contradictions. Pure docs/help/consistency
iteration; no new flags. Two deliberate shape changes, both narrowing what
iter-252 left implicit: explicit `--fields` becomes an exact projection
(E1), and a non-string `title` stays reachable (E2). Ships
before the 0.22.0 release so the compact shape and the short help go out
coherent.

Out of scope (own iterations or DEC): BUG-2 stale `set --index` refresh,
UX-3 UTF-8 placeholder, UX-5 `new --property`, COH-9 envelope unification,
FIND-8 index materialisation cost (see [[iterations/iteration-253-read-lines-single-pass]]).

## Tasks

### A. Silent wrong result [2/2]

- [x] FIND-2: add `--fields tasks` to the "planned items where all tasks are
      done" recipe in `crates/hyalo-cli/templates/skill-hyalo-tidy.md`,
      `crates/hyalo-cli/templates/pi/skills/hyalo-tidy/SKILL.md` and
      `.claude/skills/hyalo-tidy/SKILL.md`; `check-bundled-skills` /
      `check-pi-package-sync` stay green.
- [x] Add an e2e test that greps every bundled skill/template/rule file for
      `--jq` recipes referencing `.tasks`, `.sections`, `.links`,
      `.backlinks` and asserts the same command line carries `--fields`,
      `--view`, or an auto-including filter — so the next shape change cannot
      leave a silent recipe behind.

### B. Short-help sentence fragments [3/3]

- [x] COH-6/HELP-1: rewrite the first paragraph of each affected doc comment
      as one complete sentence and move the blank line to the sentence
      boundary: `set`/`append --validate`, `lint --strict`, `lint --profile`,
      `init --profile`, `new --file`, `changelog add --wrap`, `links fix
      --threshold`, `links fix --case-insensitive`, `links auto
      --no-first-only`, `--exclude-target-glob`, `--no-warn-common-titles`,
      `mv [DEST]`, `mv --allow-ambiguous`, `madr toc [DIR]`, `read -s`,
      `properties`/`tags -g` and `-n`. Verify `--help` no longer has a blank
      line mid-sentence for any of them.
- [x] HELP-2: give the shared `--file/--glob/--files-from` arg struct used by
      `read`, `task read/toggle/set`, `backlinks` the same one-line short
      help as `find` (`Target file(s), repeatable (excludes --glob /
      --files-from)`, `Glob(s) relative to --dir, repeatable; '!' negates`,
      `Read paths from PATH, one per line ('-' = stdin)`); same for `task *
      --section`.
- [x] Extend `xtask check-help-drift` with a dangling-fragment guard: walk
      every `-h` page and fail if a flag's short line ends in `,` `;` `:`
      or a dangling word (`and or by if to a the rather would (no`), or if a
      short-help entry spans more than 2 rendered lines. Add the matching
      e2e in `agent_discoverability.rs`.

### C. Stale result-shape docs [4/4]

- [x] COH-3: `find` headline (root `--help` Commands, `find -h` line 1) and
      `find --help` paragraph 1 describe the compact default shape and say
      `--fields` adds sections/tasks/links/backlinks.
- [x] COH-4: regenerate the root `--help` JSON-shape cookbook from a real
      run — `find` item `{file, modified, size, lines, title, properties,
      tags}` with `title` outside `properties`; `read` result carries
      `size`/`lines`; `task read/toggle/set` note the array form for
      multi-line ops. Add an e2e that runs `find --limit 1` and asserts the
      cookbook's key list equals the actual key list, so it cannot drift again.
- [x] COH-5: `read --help` documents `size`/`lines` and the >8 KiB
      `--lines`/`--section` hint; drop the iter-252 plan's "text mode shows
      size in the header line" claim or implement it (decide, record in DEC).
- [x] COH-16/FIND-6: `--orphan` and `--dead-end` flag lines say
      "auto-includes links and backlinks" (matches behaviour and the
      `--fields` paragraph). `--fields` help, the text `fields:` summary
      line and the "unknown field" error all state the E1 rule in one
      sentence: "Without --fields: file, modified, size, lines, title,
      properties, tags. With --fields: exactly the named fields plus file
      (filters add what they need)."

### D. Help-vs-behaviour contradictions [5/5]

- [x] HELP-3: `views run` either honours `--filenames-only`/`--filenames0`
      (preferred — route through the same projection as `find`) or removes
      them from the `views run -h` Output group; `views set -h` lists only
      flags a view actually persists (`--strict`? decide and document).
- [x] COH-10: `lint --help` example → `hyalo lint --fix --fix-rule
      HYALO001`; `--fix-rule` short line "With --fix, only autofix the
      specified rule(s) (repeatable)"; fix the same recipe in `CLAUDE.md`.
- [x] COH-1/COH-2/HELP-4: one global-options list. Root `-h` GLOBAL OPTIONS
      gains `-d, --dir <DIR>` and `--index-file`; the 52 pointer lines and
      the root `--help` COMMAND REFERENCE "Global flags" block are generated
      from the same list (drop the contradictory "Per-subcommand index
      flags" block). Consider hiding `--hints` from `-h` (HELP-9).
- [x] HELP-6/COH-11: `mv --property` short and long help → "Same syntax as
      `find --property`" (it already accepts `~=`, `!K`, dot-paths).
- [x] HELP-7: `task --line` help states file-absolute 1-based numbering
      (frontmatter counted); `read --lines` already says body-relative —
      cross-reference both.

### E. Result-shape semantics [2/2]

- [x] FIND-1 — exact projection. `file` is the only unconditional key; it
      names the result and is never droppable. `modified`, `size`, `lines`
      become ordinary members of the field set: present in the *default*
      set (no `--fields`) because they are cheap (`stat`, or already
      scanned) and are the inputs an agent uses to choose its next call
      (`read --lines`, recency), but dropped when an explicit `--fields`
      does not name them. So `--fields title` → `{file, title}`;
      `--fields size,lines` → `{file, size, lines}`; `--fields all` is
      unchanged; `--fields file` is accepted and means `{file}`. Filter
      auto-includes (`--section`→sections, `--task`→tasks, `--broken-links`/
      `--orphan`/`--dead-end`→links+backlinks, count sorts→that field) still
      add to whatever set is in force. Views with a pinned `fields` follow
      the same rule as an explicit `--fields`; CLI `--fields` on top
      replaces the pin. Implement in the one projection point that already
      builds the item (`commands/find.rs` field-set handling and the text
      renderer), not as a post-filter on the JSON. Record as DEC-254 with
      the reasoning above; update the `views --help` FIELDS paragraph, the
      `rule-knowledgebase.md` shape notes and the bundled `SKILL.md`s.
- [x] FIND-3/FIND-4 — non-string `title`. Scalars are stringified as
      written in the file: `title: 42` → `"42"`, `title: 1.0` → `"1.0"`,
      `title: 2026-08-30` → `"2026-08-30"`, `title: true` → `"true"` (the
      author meant the text; YAML's type inference is the accident). The
      typed value stays in `properties-typed`. `--property title=42` and
      `--sort title` compare that string like every other key. Null, empty
      and whitespace-only titles count as absent: H1 fallback, then
      filename. Collections (`title: [a, b]`, a map) have no honest string:
      promoted `title` falls back to H1/filename **and** the raw value is
      kept in `properties` (strip only when the promotion consumed a
      scalar), and a new HYALO lint rule warns "title must be a scalar".
      Text mode never prints `(none)` for a file that has a raw title.
      Amend DEC-252 with the rule. Tests for `title: 42`, `1.0`,
      `2026-08-30`, `true`, `[a, b]`, `{k: v}`, `title:`, `title: "  "`.

### F. Cosmetic, batch if cheap [4/4]

- [x] COH-7: `find --help` PROJECTIONS renders `\0` / `\n` as escapes, not
      literal control characters.
- [x] COH-8: move the `[crate::list_commands::LIST_COMMANDS]` rustdoc link
      out of the `--index` user-facing long help (it appears on ~15 pages).
- [x] COH-15: use `\u{00a0}` only on non-command indentation in
      `cli/args.rs` so `hyalo …` example lines paste into a shell; quote the
      `set --property tags=[a,b,c]` example; unwrap the three `--jq`
      examples that split across lines.
- [x] HELP-8/HELP-11/HELP-12: `value_name` `N`/`GLOB`/`SUBSTR`/`TITLE` on
      `links fix`/`links auto` so they render single-line like the other 50
      pages; drop the duplicated `(default: 10)` on `summary -n`; normalise
      `<GLOB>`/`<FILE>`/`<PATH>` placeholders and the nine `--dry-run`
      one-liners to one wording each.

## Acceptance criteria

- [x] The FIND-2 recipe returns the same non-empty list as
      `find --property status=completed --fields tasks --jq …` on the own
      KB, and the new recipe-shape test guards all bundled skills.
- [x] `check-help-drift` fails on a deliberately re-introduced fragment
      (test the guard by mutation in the e2e) and passes on `main`.
- [x] No short-help line on any of the 52 pages ends mid-sentence; `hyalo
      -h` stays ≤ 2560 B and every subcommand `-h` ≤ 3072 B (the 251
      ceilings still hold after B and D — measure and record the new
      numbers in the PR body).
- [x] The root `--help` JSON cookbook key lists equal the live `find`,
      `read` and `task toggle` output keys (asserted by the new e2e).
- [x] `hyalo views run <view> --filenames-only` prints one path per line,
      or the flag no longer appears on `views run -h`.
- [x] Every example line in every `--help` page executes without a clap
      usage error in a scratch KB (extend the 251 e2e that walks
      `hyalo help`).
- [x] Exact projection: `find --fields title --limit 1` JSON item keys are
      exactly `["file","title"]`; `--fields size,lines` → `["file","lines",
      "size"]`; no `--fields` → the seven default keys; `--fields title
      --section Goal` → `["file","sections","title"]`; a view pinning
      `fields = ["title"]` behaves like `--fields title`. Text mode shows the
      same set. Payload for `--fields title --limit 50` on the own KB drops
      by ≥20 % vs 0.22.0 (record the number).
- [x] `find --file <numeric-title>.md` promotes the stringified scalar; a list title
      falls back to H1 and keeps the raw value in `properties.title`; `lint` warns.
- [x] Docs in sync: README (report-only check), `rule-knowledgebase.md`,
      bundled `SKILL.md`s, `CLAUDE.md`, CHANGELOG `[Unreleased]` entries
      under Fixed/Changed.
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, all eight `xtask check-*`, `hyalo lint --strict`.

## Non-goals

- Envelope unification for `dry_run`/`skipped_count` (COH-9) — file a DEC or
  its own iteration; this one only softens the sentence in `--help`.
- Forwarding `hyalo help <cmd>` to `-h` (HELP-5) — clap behaviour change;
  decide separately. This iteration only adds `hyalo help <cmd> = --help` to
  the root `-h` "Everything else" line.
- Root `-h` example set and command-group reshuffle (LOW) — revisit after
  the next dogfood round.

## Links

- [[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]]
- [[iterations/iteration-251-agent-discoverability-help]]
- [[iterations/iteration-252-find-result-shape]]
- [[iterations/iteration-253-read-lines-single-pass]]
