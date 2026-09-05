---
title: "Iteration 275 — Alias semantics as Obsidian means them, mv ambiguity guards for every layout, anchor and capture polish"
type: iteration
date: 2026-09-05
tags: [iteration, links, mv, obsidian, dogfooding]
status: completed
branch: iter-275/alias-semantics-and-mv-guards
priority: 1
related:
  - "[[dogfood-results/dogfood-v0220-post-batch-271-274]]"
  - "[[iterations/iteration-272-resolution-completeness]]"
  - "[[iterations/iteration-271-write-and-rewrite-safety]]"
  - "[[decision-log]]"
---

# Iteration 275 — Alias semantics as Obsidian means them, mv ambiguity guards for every layout, anchor and capture polish

## Goal

Close the three HIGH resolution bugs and every MEDIUM/LOW `mv`, anchor and capture finding from
[[dogfood-results/dogfood-v0220-post-batch-271-274]] (BUG-1, 2, 3, 7, 8, 9, 10, 23, 25, 26, 31,
32, 36, 37, 39; UX-2). The batch's mechanics are sound; two of its premises are not. DEC-296
assumed Obsidian resolves a bare `[[alias]]`, and it does not (verified against the Obsidian
help page and a moderator's "intentional design decision" on the forum). The 271 ambiguity
guard for frontmatter links was tested only with the moved file at the vault root.

Rules: **no new CLI flags**; every fixture from the report becomes a unit or e2e test; a WIP
commit after each part; anything still open when the gates are green goes to `backlog/` with
the report's repro.

## Part A — Aliases mirror Obsidian (BUG-1, 2, 26, 32)

- [x] ALIAS-1: Write DEC-308 amending DEC-296. A bare `[[alias]]` is **broken** in Obsidian, so
      hyalo reports it broken by default: `via: "alias"` links count in `summary.links.broken`,
      `--broken-links`, HYALO006 and `links fix`. Record the corrected premise (help page
      wording, forum thread `wikilink-resolution-does-not-honor-frontmatter-aliases-1-12-7`,
      the Alias Linker plugin) in the DEC's Why.
- [x] ALIAS-2: The alias map's job becomes an alias-backed **fixable** plan in `links fix`:
      `[[Leah]]` → `[[Leah Ferguson|Leah]]` (label preserved when present:
      `[[Leah|label]]` → `[[Leah Ferguson|label]]`; markdown/embed forms rewrite the target
      only), confidence 1.0, its own bucket (`alias_fixes`), never routed through fuzzy,
      applied by `--apply` (not `--apply-fuzzy`). `emitted_target` on every entry. Frontmatter
      alias links get the same plan through the frontmatter rewriter.
- [x] ALIAS-3: `[links] aliases` keeps its meaning "treat a bare alias as resolved" for vaults
      running Alias Linker, but defaults to **false**; `hyalo config` reports it; `find --help`
      and the rule/skill templates describe the two modes in one sentence each. With
      `aliases = true` behaviour is exactly today's.
- [x] ALIAS-4 (BUG-2): an ambiguous stem is never tie-broken by an alias, in either mode:
      `[[avatar]]` with `Plugins/avatar.md` (alias `Avatar`) and `Themes/Avatar.md` is
      `path: null` in `find`, `backlinks`, `summary` and `mv` alike. Hub fixture: after
      `mv Plugins/avatar.md Plugins/avatar-plugin.md` the four links stay ambiguous, not
      silently attributed to the theme. Recompute the Hub's `summary.links.broken` and record
      the honest number in the Outcome (272's 154 included the tie-break).
- [x] ALIAS-5 (BUG-26): an alias collision (`aliases: [Twin]` on two notes) reports
      `candidates: [..]` under `links fix` like a stem collision, and HYALO006 says
      "ambiguous" with the candidates, not "does not resolve".
- [x] ALIAS-6 (BUG-32): text mode prints `(via alias)` after the path where `find --help`
      promises it, in both modes (in default mode it marks a broken-but-fixable link).

## Part B — `mv` ambiguity guard for every layout (BUG-3, 8, 9, 10, 25, 31, 39, UX-2)

- [x] MV-1 (BUG-3): the frontmatter ambiguity guard in `plan_frontmatter_wikilink_rewrites`
      keys on the moved file's stem regardless of its directory. Fixture matrix from the
      report (moved dir × twin dir × source dir, all seven combinations) as a parameterised
      e2e test; kepano repro (`Categories/Books.md` + `Notes/Books.md`, `categories:` list in
      two notes) as a second. `--allow-ambiguous` rewrites and lists them; the default skips
      and lists them under `skipped_ambiguous` with `property`.
- [x] MV-2 (BUG-8): the moved file's own **body** self-links (`[[a]]`, `[[a#Top]]`) go through
      the same guard as inbound links: rewritten when the old stem is unique, otherwise
      listed under `skipped_ambiguous` with `self: true`. Frontmatter self-links already
      rewrite; keep that and add the same field.
- [x] MV-3 (BUG-9): a `[[[x]], [[y]]]` frontmatter flow list is rewritten by `mv` (single-line
      span, same rewriter as the quoted form) or, if the span cannot be rewritten in place,
      listed under `frontmatter_links_skipped`; never silent.
- [x] MV-4 (BUG-10): the destination is resolved exactly like the source (DEC-304): an
      absolute path inside the vault is accepted (with the same "prefer relative" note the
      source prints); `--to kb/`, `--to ./kb/`, `--to ./` from inside the vault and `--to .`
      all mean the vault root. Fix the "create kb/ first" hint so it never names the vault
      itself.
- [x] MV-5 (BUG-25): batch `mv` dry-run reports a destination collision as a row
      (`collisions: [{source, destination}]`) instead of aborting, and its JSON carries the
      same `total_files_updated` / `total_links_updated` numbers single-file mode has.
- [x] MV-6 (BUG-39): batch `mv` prints each ambiguous-link warning once.
- [x] MV-7 (BUG-31): `frontmatter_links_skipped[].target` and every other reported link text
      is trimmed of the whitespace a split block scalar leaves.
- [x] MV-8 (UX-2): `mv` text mode prints the `property` name on `skipped_ambiguous` notes
      (`note: skipped ambiguous link [[a]] at b.md:3 (property: related)`).

## Part C — Anchors and capture polish (BUG-7, 23, 36, 37)

- [x] ANCHOR-1 (BUG-7): the DEC-298 suggestion fires when the folded fragment equals the whole
      heading (`anchor.rs:199`: `<` not `<=`), and resolution itself folds `_` the way it folds
      `-` and `%20`, so `#Browser_compatibility` resolves to `## Browser compatibility`
      outright (amend DEC-268/DEC-298 in one DEC-309 entry). Measure MDN: broken anchors
      10929 → expect a large drop; record before/after in the Outcome.
- [x] ANCHOR-2 (BUG-36): decide-or-implement nested heading paths `[[a#H1#H2]]` (Obsidian
      resolves them as a heading path). Implement if it is a resolver-only change; otherwise
      a DEC next to DEC-299 saying why not.
- [x] CAPTURE-1 (BUG-23): wikilink targets are trimmed before resolution (`[[ a ]]`,
      `[[a ]]`, `[[a #h]]`); `target` keeps the raw text, `path` is the resolved file,
      HYALO006 stops firing on them; `mv` rewrites the trimmed form. Zero Hub occurrences, so
      the fixture is synthetic.
- [x] CAPTURE-2 (BUG-37): `[[./a]]` reports the canonical `a.md`, like `[[/a]]`.

## Shared closing tasks

- [x] Changelog entries via `hyalo changelog add` (one per part, listing the bugs).
- [x] DEC-308 (aliases) and DEC-309 (anchor folding) in [[decision-log]]; DEC-296's entry
      gains a "superseded in part by DEC-308" line.
- [x] `find --help`, `links fix --help`, `mv --help`, `rule-knowledgebase.md`,
      `skill-hyalo.md` and `.claude/CLAUDE.md` updated in the same PR (alias modes,
      `alias_fixes`, `self`, `collisions`).
- [x] Every unfinished item moved to `backlog/` with its repro.
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, every xtask `check-*`
      gate (`CARGO_MANIFEST_DIR=<repo>/crates/xtask ./target/debug/xtask <gate>`).

## Acceptance criteria

- [x] On the Hub with default config, `[[Leah]]` is broken and `links fix --dry-run` lists it
      under `alias_fixes` with `emitted_target: "[[Leah Ferguson|Leah]]"`; `--apply` on a
      scratch copy writes exactly that; with `[links] aliases = true` today's resolution holds.
- [x] `[[avatar]]` on the Hub is `path: null` everywhere; `mv Plugins/avatar.md …` on a copy
      leaves the four links ambiguous and lists them.
- [x] The seven-combination fixture and the kepano repro pass: no frontmatter link is
      rewritten or dropped silently for any layout; `--allow-ambiguous` rewrites them all.
- [x] `mv kb/a.md kb/z.md` with `kb/sub/a.md` present lists the two body self-links under
      `skipped_ambiguous` with `self: true`; without the twin they are rewritten.
- [x] `[[#predefined_fallback_options]]` resolves to `## Predefined fallback options`; the
      MDN broken-anchor count is recorded before and after.
- [x] `mv sub/a.md /abs/vault/a3.md` and `mv kb/sub/a.md --to kb/` both work.
- [x] Gates green; changelog; DECs.

## Outcome

Every planned item shipped; nothing went to `backlog/`.

**Part A — aliases (DEC-308).** `[links] aliases` now defaults to `false`, so a bare
`[[alias]]` is broken the way Obsidian renders it. The alias map is still built in both
modes — it is what `links fix` and the `via: "alias"` label read. On the **Obsidian Hub**
with the default config: `links fix --dry-run` reports `alias_fixes: 8` (was: 51 links
silently "resolved" and unfixable), `broken: 54`, `ambiguous: 100`, `fuzzy: 18`, and
`summary.links.broken` is **154 files of 22 400 links** — the same file count as iteration
272 reported, because the files that gained newly-broken alias links already held another
broken link. The link-level picture is what changed, not the file-level one.

`emitted_target` is the note's **vault-relative path** plus the alias label —
`01 - Community/People/Leah Ferguson|Leah`, rendering `[[01 - Community/People/Leah
Ferguson|Leah]]`. The plan's acceptance criterion wrote the flat-vault form
(`Leah Ferguson|Leah`), which is exactly what this produces when the note sits at the vault
root. Path form is deliberate: it is what every other wikilink fix emits, it round-trips
through the H-1 guard, and it cannot become ambiguous if a second `Leah Ferguson.md` appears.

ALIAS-4 verified on the Hub: `[[avatar]]` ×3 and `[[Avatar]]` all report `path: null` and
`via: null` — an ambiguous filename match is a filename match, and the alias on
`Plugins/avatar.md` no longer breaks the tie for `find` while `mv` calls it ambiguous.

**Part B — `mv`.** The frontmatter ambiguity guard now fires for the full moved-dir × twin-dir
× source-dir matrix (a parameterised e2e test walks all of it), plus the kepano repro. The
moved file's own body self-links are guarded and reported with `self: true`; a `[[[x]], [[y]]]`
flow list rewrites; absolute in-vault destinations and all four vault-root spellings resolve;
a batch dry run lists `collisions` instead of aborting and carries the single-file counters;
each ambiguous warning prints once; split-link targets are trimmed; text mode prints
`(property: …)`.

**Part C — anchors and capture.** `#Browser_compatibility` resolves to `## Browser
compatibility` (DEC-309). Measured on the MDN checkout (`../mdn/files/en-us`):
**10 929 broken anchors before → 529 after**, a 95 % drop, with no change to the broken-target
count. Nested heading paths were *implemented* rather than deferred (DEC-311) — the check is
resolver-only, since `OutlineSection` already carries `level` and document order. Wikilink
targets are trimmed and `.` segments dropped (DEC-310).

**Decisions:** DEC-308 (aliases, amending DEC-296), DEC-309 (separator folding on resolution,
amending DEC-268/DEC-298), DEC-310 (target trimming and `./`), DEC-311 (nested heading paths,
beside DEC-299). DEC-296 gained a "superseded in part by DEC-308" addendum.

**No new CLI flags.** Every change is a default, a report field or a resolution rule.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-271-274]] — BUG-1, 2, 3, 7, 8, 9, 10, 23, 25, 26, 31, 32, 36, 37, 39; UX-2
- [[iterations/iteration-272-resolution-completeness]] — DEC-296, DEC-298 as shipped
- [[iterations/iteration-271-write-and-rewrite-safety]] — Part G, the ambiguity guard
- [[decision-log]] — DEC-268, DEC-288, DEC-296, DEC-298, DEC-299, DEC-304
