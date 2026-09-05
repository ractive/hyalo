# Configuration reference

> Part of the [hyalo](../README.md) documentation.

`hyalo init` creates a `.hyalo.toml` in your project root. All fields are optional — CLI flags always take precedence.

```toml
dir = "./my-vault"        # vault directory (default: ".")
format = "text"           # output format: "json" or "text" (default: TTY-aware — text on terminals, json when piped)
hints = false             # drill-down command hints (default: true)
default_limit = 100       # max results for list commands (default: 50; 0 = unlimited)

[links]
frontmatter_properties = ["related", "depends-on"]   # list properties that contribute to the link graph
case_insensitive = "auto"                             # "auto", "true", or "false"
aliases = true                                        # frontmatter `aliases:` resolve wikilinks (default: true)
fuzzy_min_confidence = 0.8                            # confidence floor for links fix --apply-fuzzy (default: 0.8)

[links.auto]
exclude_titles = ["permissions", "README"]            # titles hyalo links auto never links
exclude_target_globs = ["templates/*"]                # pages hyalo links auto never links to
first_only = true                                     # link only the first mention per file
warn_common_titles = true                             # note when a candidate title looks noisy

[schema.default]
required = ["title"]

[schema.types.iteration]
required = ["title", "date", "status", "tags"]
filename-template = "iterations/iteration-{n}-{slug}.md"

[schema.types.iteration.properties.status]
type = "enum"
values = ["planned", "in-progress", "completed", "superseded"]
```

## Config resolution: which `.hyalo.toml` applies

hyalo reads **one** `.hyalo.toml` per run — the one in the current working
directory. It does not merge configs and does not walk up the directory tree.
`dir` inside that file names the vault, resolved relative to the config file, so
the standard layout is a config at the repo root and a vault in a subdirectory:

```text
my-project/
  .hyalo.toml        # dir = "kb"  → this is the config in effect
  kb/                # the vault
```

### What `--dir` does

| Invocation | Config in effect | Vault |
| --- | --- | --- |
| `hyalo lint` (from `my-project/`) | `my-project/.hyalo.toml` | `kb/` |
| `hyalo lint --dir kb` | `my-project/.hyalo.toml` | `kb/` |
| `hyalo lint --dir other` | `other/.hyalo.toml` if it exists, else built-in defaults | `other/` |

`--dir` names a **vault**, not a config. When it resolves to the directory the
config already points at, the config keeps applying and hyalo emits a one-time
`note:` that the flag is redundant. When it names a different tree, the CWD
config no longer applies and hyalo says so on stderr, naming the config file
that took over (or reporting that it is running on built-in defaults).

> **Changed in 0.21.0.** `--dir <configured-vault>` used to *discard* the
> config: schema, saved views, `[lint] ignore`, per-rule severity overrides and
> `site_prefix` were all silently dropped, which could turn a `lint --strict` CI
> gate vacuously green. Run `hyalo config --dir <path>` to see exactly which
> file is in effect for any invocation.

### When `.hyalo.toml` cannot be parsed

An unknown key or a type error anywhere in the file — including inside
`[links.auto]` or `[schema.*]` — makes the whole file unusable. hyalo then:

- prints the parse diagnostic (with line, column and the accepted key names).
  This warning is **not** suppressed by `--quiet`: a config that stopped
  applying changes which vault and which rules a command uses, so it is not
  chatter;
- **refuses mutating commands** (`set`, `remove`, `append`, `mv`, `new`,
  `task toggle`, `views set`, `links auto --apply`, `lint --fix`, …) with exit
  code 1, writing nothing. `--dry-run` invocations are unaffected;
- lets read-only commands continue on built-in defaults, keeping the `dir` value
  if it can still be recovered from the file, so reads stay inside the vault you
  configured.

`hyalo init` and `hyalo deinit` are never blocked — they are how a broken config
gets repaired.

### When only `[schema]` cannot be loaded

A `[schema]` section can be valid TOML and still be rejected — an uncompilable
regex, `min-length` on a `number` property, `values` outside an `enum`. The file
itself parses, so `dir`, `[lint] ignore` and the saved views all still apply;
only the schema is gone, replaced by an empty one. hyalo then:

- prints the `invalid [schema] in .hyalo.toml: …` diagnostic, again `--quiet`-proof;
- reports it as a `schema/malformed` violation from `hyalo lint`, so a
  `lint --strict` gate fails rather than passing against no schema at all;
- **refuses `set` / `append` when validation was requested** — `--validate`, or
  `[schema] validate_on_write = true` — with exit code 1, writing nothing,
  because validating against an empty schema rejects nothing and the flag's
  guarantee would be silently vacuous. `--dry-run` refuses too: it is the same
  claim without the write;
- leaves everything else alone. A `set`/`append` without `--validate` promises no
  validation and still writes (warned), and every other command — `mv`,
  `remove`, `task toggle`, all reads — is unaffected, so a vault mid-schema-edit
  stays usable.

## Case-insensitive link resolution

`[links] case_insensitive` controls whether a link whose target differs only in
casing (`[[Foo]]` → `foo.md`) still resolves:

| Value | Behaviour |
| --- | --- |
| `"auto"` (default) | Detect the filesystem's case behaviour once per run and follow it |
| `"true"` | Always resolve case-insensitively |
| `"false"` | Never resolve case-insensitively — casing must match exactly |

Instead of (or in addition to) the scalar value, a `[links.case_insensitive]`
sub-table is accepted:

```toml
[links.case_insensitive]
resolve = true
```

The sub-table form always enables the case-insensitive fallback **and** treats
case-fold-resolving targets as *resolved* rather than fixable: `hyalo links fix`
reports no `link-case-mismatch` rewrites for them. Use this on MDN-style vaults
whose case-folded directory layouts (`en-US` written as `en-us`) otherwise make
a dry run offer tens of thousands of casing rewrites that every downstream
tool would resolve fine anyway. The same effect for a single run is the
`links fix --case-insensitive` flag.

### `[links] aliases` — frontmatter aliases as link targets

A note's frontmatter `aliases:` are alternative names it can be linked by, the
way Obsidian treats them:

```yaml
---
title: Leah Ferguson
aliases:
  - Leah
  - L. Ferguson
---
```

`[[Leah]]` written anywhere in the vault then resolves to that note. The rules
(DEC-296):

- Only the `aliases` property is read, in either shape Obsidian writes — a list
  or a bare string (`aliases: Leah`).
- **A filename or path match always wins.** The alias map is consulted only
  after every path, `.md`-suffix, directory-index and bare-stem attempt fails,
  so no existing link is ever repointed by someone else's frontmatter.
- An alias declared by **two** notes is ambiguous and resolves to nothing —
  the same verdict a colliding bare stem gets.
- Matching folds case, like every other lookup, and `[[alias#Heading]]` /
  `[[alias|label]]` work.
- A link that resolved this way reports `via: "alias"`; its `kind` stays
  `wikilink`. It is a real graph edge, so `backlinks`, `--orphan`,
  `--dead-end`, `summary.links` and HYALO006 all agree with
  `find --fields links`.
- `links fix` never proposes a rewrite for such a target and never
  fuzzy-matches one; `mv` leaves alias-written links alone, because the alias
  travels with the note.

Set `aliases = false` to restore filename-only resolution.
`hyalo config --jq '.results.links.aliases'` reports the effective value.

Under `"auto"`, hyalo detects case behaviour with **stat calls only**: it looks
up an existing vault entry (or the vault directory itself) under a
case-flipped name and checks whether it lands on the same object. Read-only
commands therefore write nothing into the vault — no probe file, no directory
mtime change.

The one case that still needs a write is a vault that offers no usable
candidate at all: an empty directory whose own path has no ASCII letters. Then
hyalo falls back to creating and deleting a short-lived `.hyalo-case-probe-*`
file in the vault root. **If that write fails — a read-only mount, for example
— case-insensitive resolution silently turns off** and links are resolved
case-sensitively, which can report casing-mismatched links as broken. Set
`case_insensitive = "true"` explicitly when working against a read-only vault
on a case-insensitive filesystem.

If a process is killed between creating and deleting a fallback probe file,
the orphan is dot-prefixed and invisible to `hyalo find`; the next
`hyalo create-index` run sweeps `.hyalo-case-probe-*` files older than a
minute from the vault root.

## Persistent auto-link exclusions

`hyalo links auto` links unlinked mentions of page titles. On a vault whose
titles double as common words (`permissions`, `index`, `README`) most
candidates are noise, and the flags that suppress them had to be retyped on
every invocation. `[links.auto]` persists them per vault:

```toml
[links.auto]
exclude_titles = ["permissions", "README", "index"]   # like --exclude-title (case-insensitive)
exclude_target_globs = ["templates/*"]                # like --exclude-target-glob (case-insensitive)
first_only = true                                     # like --first-only
warn_common_titles = false                            # opt out of the noisy-title note
```

Merge rules with the CLI flags:

| Setting | With flags |
| --- | --- |
| `exclude_titles` | **Unioned** with `--exclude-title` — the flag extends the config list, never replaces it |
| `exclude_target_globs` | **Unioned** with `--exclude-target-glob` |

Both exclusion lists match **case-insensitively**. `exclude_titles = ["readme"]`
excludes a page titled `README`, and `exclude_target_globs = ["templates/*"]`
excludes `Templates/Note.md` — so a vault whose directory casing is
inconsistent (or which lives on a case-insensitive filesystem) does not need a
pattern per spelling.
| `first_only` | `--first-only` turns it on for a single run; the config turns it on for every run; `--no-first-only` turns it off for a single run |
| `warn_common_titles` | On by default; `--no-warn-common-titles` turns it off for a single run, the config for every run |

`first_only` is the one key with a counter-flag: `--no-first-only` forces
first-mention-only **off** for a single run, so a vault that persists
`first_only = true` can still get a one-off all-mentions pass without editing
`.hyalo.toml`. It conflicts with `--first-only` — passing both is an error
rather than a silent precedence puzzle — and is a no-op when the config does
not enable `first_only`. The list keys have no counter-flag: they are unioned,
and narrowing a run's scope with `--file`/`--glob` covers the same need.

Because config exclusions apply silently, a run whose candidates they removed
reports how many: `config_excluded` in the JSON envelope (omitted when zero,
like `links.out_of_vault`) and an `Excluded by [links.auto] config: N titles`
line in text output. `hyalo config` prints the effective settings as
`links.auto.exclude_titles` / `links.auto.exclude_target_globs` /
`links.auto.first_only` / `links.auto.warn_common_titles` (and
`results.links_auto` in JSON).

### The noisy candidate title note

Exclusions only help once you have noticed the noise. On a first run, `hyalo
links auto` also looks at the links it is about to propose and, if any of them
come from a title that looks like a source of over-linking, prints one advisory
line on stderr:

```text
note: 3 auto-link candidate titles are common English words or unusually frequent and
account for 588 of 1179 proposed links (showing the 5 noisiest of 7): "Workflows"
(502×, 43%, unusually frequent), "limits" (46×, common English word), "runner groups"
(45×, 4%, unusually frequent). If those are prose mentions rather than deliberate
references, skip them with --exclude-title Workflows --exclude-title limits
--exclude-title 'runner groups' — or persist them once under [links.auto]
exclude_titles in .hyalo.toml. Silence this note with --no-warn-common-titles.
```

Two independent triggers put a title in that list:

- **Common English word.** The title is an ordinary English word or a generic
  doc filename (`permissions`, `index`, `notes`, `README`). The list lives in
  `hyalo-core::common_words` — bundled, no dependency — and covers regular
  plurals, so `permissions` matches the stored `permission`. Titles shorter
  than 3 characters are never classified; `--min-length` already excludes them.
  Being an English word list, it only ever matches ASCII titles.
- **Unusually frequent.** The title produced at least **25** proposed links
  *and* at least **2.5%** of the run — `max(25, ceil(total / 40))` matches, so
  the absolute floor governs runs below 1,000 links and the share governs
  larger ones. This trigger is purely arithmetic and therefore
  language-independent: a German or Japanese vault gets the note too.

A title flagged by both is labelled as both. When every offender was flagged
for the same reason the note states it once in the opening clause instead of
repeating it per entry.

Properties of the check, all deliberate:

- **stderr only.** The report on stdout is byte-identical with the note enabled
  or disabled, so no JSON consumer or diff-based workflow changes.
- **Self-extinguishing.** Only titles that actually produced matches in *this*
  run are named. Excluding them — by flag or config — removes the note.
- **Match-count honest.** The counts quoted are the links being offered, not an
  estimate over the title inventory.
- **One paste-back is enough.** The prose list stops at the five noisiest and
  says so (`showing the 5 noisiest of 7`), but the suggested `--exclude-title`
  flags cover *every* offender.
- **Spelled the way your vault spells it.** A title is displayed in its most
  frequent original casing (`README`, not `readme`), while matching and
  exclusion stay case-insensitive.
- **Silenced by `-q`** like every other note, or permanently by
  `warn_common_titles = false`. One key governs both triggers; there are no
  configurable thresholds.

Because the share is measured against *this* run, excluding a dominant title
can bring the next tier above the threshold: on a large vault the second run
may name titles the first one did not, simply because the run it is measured
against is now much smaller. That converges rather than nagging — every round
removes at least 25 links, and once nothing clears the 25-match floor the note
stops for good.

`hyalo links fix` has a similar per-invocation filter spelled
`--ignore-target <substring>`. It matches link *targets* by substring rather
than page titles or paths, so it is deliberately not part of `[links.auto]` and
keeps its own name.

## Fuzzy-fix confidence floor

`hyalo links fix` never writes a low-confidence guess under a plain `--apply`.
Opting in with `--apply-fuzzy` clears the first gate; the second is a
**confidence floor**, `0.8` by default.

Every proposal in the low-confidence bucket carries a score in `0.0`–`1.0`:

- **70%** the final path segment — a soft token match over the slug, so
  `actions-limits` no longer looks like `actions` just because they share a
  prefix, while a typo (`configuraton` → `configuration`) still scores high.
- **30%** the directory path, three quarters of which is *shared leading
  components*. A relocation inside a section (`a/b/c/page` → `a/b/d/page`)
  therefore scores far above a same-name substitution across sections
  (`/actions` → `graphql/reference/actions.md`, which lands on exactly `0.7`).
- A target written with no directory at all (`[[page]]`) asserts no location,
  so only its basename is scored.

Move the floor per run with `--min-confidence <0.0-1.0>` (which also implies
`--apply-fuzzy`), or per vault:

```toml
[links]
fuzzy_min_confidence = 0.9   # only near-certain guesses are written
```

The flag wins over the config key, and the config key wins over the built-in
default. Setting the key never opts *in* to applying fuzzy fixes — that still
requires `--apply-fuzzy`. `--min-confidence 0` restores the pre-0.21
accept-everything behaviour.

Proposals below the floor are still reported: `fuzzy_below_floor` counts them
in JSON and the text report marks each one `— below floor`. `hyalo config`
prints the floor in force as `links.fuzzy_min_confidence`.

Measured on the GitHub Docs corpus (3,710 files, 6,099 broken links), scoring
proposals against the `redirect_from` metadata GitHub maintains as ground
truth:

| floor | rewrites applied | provably wrong | correct |
| ----- | ---------------- | -------------- | ------- |
| none (pre-0.21) | 4,659 | 804 | 82.2% |
| 0.75 | 3,111 | 39 | 98.7% |
| **0.8 (default)** | **2,253** | **15** | **99.3%** |
| 0.9 | 312 | 0 | 100% |

## Agent integration (`[pi]`)

Hyalo ships a pi coding-agent extension (`hyalo init --pi`) that registers a
generic `hyalo` tool plus four typed tools — `hyalo_find`, `hyalo_read`,
`hyalo_set`, and `hyalo_task` — which take structured parameters instead of
CLI argv for the most common operations (search/filter, read, frontmatter
mutation, task toggling); the generic tool remains the escape hatch for
everything else. It also registers slash commands and a post-write lint
guardrail: when pi's
`write`/`edit` tools touch a `.md` file inside the vault, the extension runs
`hyalo lint <file>` on it and appends any violations to the tool result, so
schema drift cannot land silently.

The `[pi]` section configures the extension:

```toml
[pi]
session_summary = true   # inject a vault summary into the LLM context at session start
```

- **`session_summary`** (default `false`) — on session start, run
  `hyalo summary` once and inject the snapshot (file counts by directory,
  link health, task and status totals) into the agent's context as a hidden
  message. The agent starts every session already knowing the vault's shape
  instead of spending tool calls rediscovering it. Costs a few hundred
  tokens of context per session, which is why it is opt-in.

`hyalo config` reports the effective values as `pi.session_summary` (text)
and under `results.pi` (JSON).

## Schemas

Schemas support typed properties (`string`, `date`, `datetime`, `datetime-tz`, `number`, `boolean`, `list`, `enum`, `string-list`, `object-list` — with regex patterns, enum values, length bounds, and per-key constraints on lists of maps), per-type filename templates, path-bound types (`[[schema.bind]]`) that apply a schema to a subtree without explicit `type:` frontmatter, and reserved-file exemptions (`[schema] exempt`). Manage schemas from the CLI with `hyalo types list|show|set`, validate with `hyalo lint`, and inspect the resolved configuration with `hyalo config`.

## Saved views

Name a filter set once, recall it everywhere:

```sh
hyalo views set drafts --property status=draft
hyalo find --view drafts                          # recall
hyalo find --view drafts --tag rust               # extend with additional filters
```

## CWD-aware behaviour

When you run hyalo from a directory that has a `.hyalo.toml`, it becomes _context-aware_:

- **`hyalo --help`** prepends a short banner confirming which vault `dir` is active — useful when working from shell history or AI agent loops. Banner emojis (`ℹ️ `/`⚠️`) are TTY-gated: piped output is plain text.
- **`hyalo --version`** appends `(kb dir: <dir>)` so the resolved directory is visible at a glance. The base version string also includes the git short-sha and commit date when hyalo was built from a checkout — e.g. `hyalo 0.20.0 (abc123def456 2026-05-26)`. A `+dirty` suffix marks builds made with uncommitted changes. Set `CARGO_HYALO_FORCE_NO_GIT=1` at build time to force the bare semver form.
- **`hyalo summary`** includes the resolved `kb dir:` as its first output line. The `--format json` envelope exposes the same value as a top-level `dir` field alongside `total`, `tags`, `properties`, etc.
- **`hyalo config`** prints the full resolved configuration — handy for debugging `.hyalo.toml` resolution or feeding config into an LLM context. `--format json` uses the standard envelope, so `hyalo config --jq '.results.dir'` works like it does everywhere else; the config's own hints switch is reported as `results.hints_enabled` so it never collides with the envelope's `hints` array. `dir` is also hoisted to the envelope root.
- Running from _inside_ the vault directory emits a warning banner suggesting you `cd ..` to the project root so hyalo can find `.hyalo.toml`.
- Passing `--dir <path>` when it already matches `.hyalo.toml` emits a one-time `note:` that `--dir` is redundant. The config still applies — see [Config resolution](#config-resolution-which-hyalotoml-applies).

## Drill-down hints

Commands append a short list of follow-up commands unless `hints = false` (or
`--no-hints`) is set. Read-only suggestions use `->`; a suggestion that would
**write** to the vault or to `.hyalo.toml` uses `=>` and is tagged `[writes]`:

```text
  -> hyalo find --property status=draft --tag rust  # Narrow by tag: rust (3 files)
  => hyalo views set draft-rust --property status=draft --tag rust  # Save this query as a view [writes]
```

In `--format json` every hint object carries a boolean `writes` field, so an
agent can execute the read-only ones unattended:

```sh
hyalo find --tag rust --format json --jq '[.hints[] | select(.writes | not) | .cmd]'
```

## Snapshot index

For workflows that run many queries in a short window (CI, automation, LLM tool loops):

```sh
hyalo create-index          # one scan → .hyalo-index
hyalo find --index ...      # instant queries, no disk scan
hyalo drop-index            # clean up
```

Mutations with `--index` patch the index in-place, keeping it current for subsequent queries — and hyalo suggests creating an index automatically once a vault grows past ~500 files.

Every command documents its flags and semantics in detail: `hyalo <cmd> --help`.
