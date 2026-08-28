---
title: Deep review 2026-08-27 — code quality, security, help coherence, README, dogfood
type: review
date: 2026-08-27
status: resolved
tags:
  - review
  - security
  - ux
  - help
  - dogfooding
related:
  - "[[reviews/adversarial-review-2026-08-23]]"
  - "[[reviews/codebase-review-2026-08-06]]"
  - "[[dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups]]"
  - "[[iterations/iteration-246-help-coherence-review-followups]]"
---

# Deep review — hyalo 0.20.0 (2026-08-27)

Binary under review: `hyalo 0.20.0 (91b23dfe4d31 2026-08-27)`, built from
`main`. Method: static reading of the CLI/core/mdlint crates, mechanical diff
of every subcommand's real `--help` flags against the top-level COMMAND
REFERENCE, live adversarial probing with crafted files, full quality-gate run,
and a dogfood session against this vault plus a synthetic 10k-file vault.

Verdict up front: **the codebase is in very good shape.** Quality gates are
green (fmt, `clippy --all-targets -D warnings`, 4117 tests), the security
posture against malicious vault content is strong, and the dogfood experience
is smooth. The findings are concentrated in **help-text coherence** — the
top-level COMMAND REFERENCE has drifted from the real CLI surface in several
places.

## Findings

### F-1: COMMAND REFERENCE documents `summary --limit N` — the flag does not exist (HIGH)

Top-level `hyalo --help` shows:

```text
Summary (vault overview, read-only):
  hyalo summary [-g/--glob G] [-n/--recent N] [--depth N] [--limit N]
```

But `hyalo summary --limit 5` is a hard parse error ("unexpected argument
'--limit' found"). Worse, `summary`'s *own* subcommand help contains an
explicit FLAG NOTE explaining that summary has no `--limit` and `-n` means
`--recent` there — so the top-level reference contradicts the subcommand help
it aggregates. There is even a help test
(`summary_help_documents_short_n_divergence`) that locks in the subcommand
side of this contradiction while the COMMAND REFERENCE side is untested.

### F-2: COMMAND REFERENCE changelog synopsis uses positional args the CLI rejects (HIGH)

Top-level help:

```text
hyalo changelog add <CATEGORY> <TEXT>     Append an entry under `## [Unreleased]`
hyalo changelog release <VERSION>         Rotate ...
```

`hyalo changelog add Added "text"` → parse error. The real form is
`hyalo changelog add --category Added --message "text"` (only `release` takes
a positional `<VERSION>`). Anyone — human or agent — copy-pasting the
reference example fails on first contact.

### F-3: COMMAND REFERENCE `okf log <TEXT>` synopsis is wrong (MEDIUM)

Reference shows `hyalo okf log <TEXT> [--apply]`; the real signature is
`--message <TEXT> [TARGET]`. Same failure mode as F-2.

### F-4: Real flags missing from COMMAND REFERENCE synopses (MEDIUM)

Mechanical diff of each subcommand's `Options:` block against the top-level
reference found these real, parseable flags absent from the reference:

- `links fix`: `--apply-fuzzy`, `--min-confidence`, `--case-insensitive`,
  `--expand-short-form` (the reference only shows `--apply --threshold
  --ignore-target` for fix; `fix --help` documents all of them well)
- `task`: `--all` (only `--line`/`--section` implied via the synopsis)
- `find`: `--language`/`--stemmer` (documented extensively in `find --help`,
  absent from the reference line)
- `links auto`: several flags listed, fine; but note `--exclude-title` is in
  the reference while the auto help is the authority — verified consistent.

The reference is hand-maintained prose in `cli/help.rs`, and nothing asserts
it against the real clap surface. It has drifted, and will drift again. A
mechanical test (like the hint execution gate, but for help synopses) would
close this class permanently.

### F-5: `hyalo read` misreports invalid UTF-8 as "exceeds 1 MiB per-line limit" (MEDIUM, real bug)

A 36-byte file whose body contains invalid UTF-8 (`\xff\xfe`) makes
`hyalo read <file>` emit:

```json
"content": "<line skipped: exceeds 1 MiB per-line limit>"
```

Root cause: `scanner::read_line_capped` treats "invalid UTF-8 in chunk" the
same as "line exceeded quota" — both return `truncated = true`
(crates/hyalo-core/src/scanner/mod.rs:588-597). `read` then substitutes
`oversized_line_placeholder()` (crates/hyalo-cli/src/commands/read.rs:189).
The message names the wrong cause and the wrong limit. A user with a
latin-1-encoded or binary-contaminated note gets told their line is too long,
which sends them looking in the wrong place entirely. The scanner-level
multi-visitor path handles this correctly (lossy conversion with U+FFFD,
scanner/mod.rs:98-116) — only the `read` single-file path has the bug.
Fix: distinguish the two conditions in the `read_line_capped` return (e.g. a
small enum instead of a bool) and use a separate placeholder text.

### F-6: README: "Every write command supports --dry-run" — `hyalo new` does not (LOW)

README "Quick start" claims every write command supports `--dry-run`;
`hyalo new --help` has no such flag. Harmless (new refuses to overwrite
existing files, so there's little to preview), but the sentence is false as
written. `views set` and `types set`-without-`--dry-run`-default are other
edges; suggest rewording to "write commands that modify existing files support
--dry-run".

### F-7: `hints` vs COMMAND REFERENCE coverage gap in the execution gate (LOW, process)

The hint execution gate (`tests/e2e/hint_execution.rs`) is excellent — every
harvested hint is actually executed against a fresh vault. But its
SEED_COMMANDS list does not exercise `changelog`, `okf`, or `madr` at all, so
hints emitted by those generators are not covered by the "every hint runs"
invariant. (I verified manually that okf hints use the correct syntax; the
risk is regression, not a current bug.) Also no test executes the *help text*
cookbook/reference examples — F-1..F-3 would have been caught by one.

## Security assessment — maliciously crafted files

Probed live against the release binary. Threat model: a vault cloned from an
untrusted source (poisoned .md files, poisoned `.hyalo-index`), read and
mutated by hyalo.

**Defences that held up under probing:**

- **YAML alias bomb** (billion-laughs shape): rejected instantly with a clear
  "anchors/aliases not supported" error (6 ms). The `serde-saphyr` parser is
  configured to refuse anchors outright.
- **Deeply nested YAML** (~5000 levels under the 64 KiB budget): refused with
  "nests too deeply for hyalo's parser limits". Frontmatter is hard-budgeted
  at 64 KiB / 2000 lines (`MAX_FRONTMATTER_BYTES`, `MAX_FRONTMATTER_LINES`).
- **YAML injection via property value**: `hyalo set --property 'title=a: b\nevil:
  true'` produces correctly quoted/escaped output (`title: "a: b\nevil: true"`)
  that round-trips exactly. Multiline values get block scalars. No structure
  escape possible through values.
- **Write through parse-hostile frontmatter**: refused — a file that fails
  frontmatter parsing is never rewritten, so `set` cannot corrupt a file it
  can't fully model. Combined with the minimal-diff splice (unchanged keys
  re-emitted byte-for-byte), this is the right conservative posture.
- **Path traversal**: `../`, absolute paths, and `--files-from` entries
  pointing outside the vault are all refused or skipped with the uniform
  "resolves outside vault boundary (vault: …)" message plus a self-healing
  hint.
- **Symlink escape**: a vault symlink pointing at `/etc/hosts` is refused for
  both `read` and `set` after canonicalization (write path resolves symlink
  chains with a 32-hop cap, then boundary-checks).
- **Index writes outside the vault**: `create-index --output /tmp/x` and
  `drop-index --path /tmp/x` both require `--allow-outside-vault`.
- **Crafted snapshot index**: `SnapshotIndex::load` does defense-in-depth
  validation (SEC-1/2/3 in index.rs): NUL/absolute/parent-dir/Windows-ADS
  paths, entry-count caps (5M), graph/postings caps (50M), version skew —
  any violation falls back to a disk scan with a warning instead of trusting
  the data. Snapshot loading is fuzzed (`fuzz_targets/snapshot_loader.rs`).
- **jq resource exhaustion**: documented limits are real — `[range(3e8)]`
  errors after 3 s with a clean message; output caps (1M values / 10 MiB)
  exist. No hang, no OOM.
- **Hint injection via hostile filenames**: filenames containing `$()`, `;`,
  spaces, or single quotes are emitted in hints with correct POSIX shell
  quoting (`'it'\''s.md'`), so a copy-pasted hint can't be turned into command
  execution. Verified with `evil$(touch pwned).md` — no execution, hint is
  safely quoted.
- **ReDoS**: user regexes go through the `regex` crate (no backtracking), and
  auto-link title matching is literal string matching, not regex-built-from-
  title — a title of `a.*b` matched only its literal occurrence.
- **Fuzzing infrastructure exists and is real**: four libFuzzer targets
  (scanner, frontmatter splice, link parser, snapshot loader) with seeds and a
  corpus, deliberately isolated from the workspace so it never blocks CI.

**Residual concerns (none currently exploitable as found):**

- **S-1**: Only 2 `unsafe` blocks in the whole tree (`libc::kill(pid, 0)` for
  index lock-liveness, and a broken-pipe signal handler) — both reviewed,
  both fine. `panic = "abort"` in release means any missed panic is a hard
  crash; acceptable for a CLI, and no non-test `unwrap()`/`expect()` on
  attacker-controlled paths was found (all hits are in `#[cfg(test)]` blocks).
- **S-2**: Stale-index detection (iter-241) is mtime-heuristic-based (shallow
  directory mtimes + 1 s tolerance), not content-addressed. It caught my
  in-place edits in practice, but a same-second edit or a filesystem with
  coarse mtime granularity could slip through. The warning is a warning, not
  a fallback — `find --index` still serves stale data after warning. That's a
  documented trade-off, but agents scripting against `--index` should know
  stale results still exit 0.
- **S-3**: `--files-from` silently skips out-of-vault entries (with a hint)
  rather than erroring — reasonable for pipeline use, but a CI gate built on
  it could go vacuously green if the upstream list is wrong. The hint is the
  only signal in JSON mode.

## Code quality

- **Gates**: `cargo fmt --check` clean; `cargo clippy --workspace
  --all-targets -- -D warnings` clean (with the pedantic group enabled!); 4117
  tests pass. The workspace lints config documents each allowed pedantic lint
  with a rationale — unusually disciplined.
- **Architecture**: three crates with clean boundaries (hyalo-core: parsing/
  scanning/index; hyalo-mdlint: lint engine + profiles; hyalo-cli: CLI
  surface). Streaming-first I/O (line-capped readers, SIMD memchr splitting),
  parallel scanning via rayon, per-line memory bounds. Comments consistently
  explain *why*, cite iteration numbers and review findings (iter-202 L-16,
  SEC-1, M-6…), which makes the archaeology tractable.
- **Hotspots worth watching**: `hints.rs` is 5,059 lines, `lint.rs` 4,005,
  `output.rs` 3,744, `args.rs` 2,690 — all functional but past the size where
  navigation stays cheap. The COMMAND REFERENCE in `cli/help.rs` being hand-
  maintained prose is the direct cause of F-1..F-4.
- **Error handling**: uniform anyhow + Context, no `unwrap()` outside tests,
  structured JSON errors with hints, exit codes documented (1 user / 2
  internal) and enforced.

## Help coherence summary

Subcommand-level `--help` is excellent — thorough, example-rich, accurate on
every flag I probed. The failures are all in the **top-level aggregated
COMMAND REFERENCE** (F-1..F-4), which is the surface most likely to be read
first. Recommendation: generate the reference from clap's own surface (or add
a test that walks every subcommand's real flags and asserts presence in the
reference), and extend the hint execution gate's seed list to changelog/okf/
madr plus a pass that *runs* every cookbook example (they all currently parse
— verified all 57 — but only the hint side is locked by tests).

## README assessment

Accurate and genuinely user-readable: honest install matrix (brew/apt/dnf/AUR/
Scoop/winget/cargo/tarball), a feature table that matches reality, working
quick-start commands (boolean BM25, dot-path filters, `--filenames-only`
pipelines — all verified live). The "10,000+ files in under a second" claim
holds: 10k-file vault, `find --property` count in 0.31 s, full-text BM25 in
1.05 s cold / 0.26 s with index, `summary` in 0.44 s. Two blemishes: the
`--dry-run` overclaim (F-6) and the pi-integration section referencing
`@v0.21.0` tags that don't exist yet for this repo version (0.20.0) — mild
forward-dating that could confuse.

## Dogfooding notes (issues hit while using it)

- `hyalo types show review` → "type 'review' not found" while several files
  under `reviews/` carry `type: review`. The vault and its schema have
  drifted; `reviews/` is in `[lint] ignore` so nothing notices. (This review
  uses `type: research` to stay schema-clean.)
- The stale-index path: works, warns, but still serves stale results with
  exit 0 (S-2). For an agent that just mutated files *without* `--index`, a
  follow-up `--index` query can silently contradict disk. Would prefer an
  opt-in `--strict-index` that falls back to disk on staleness.
- `summary` text output starts with `kb dir: hyalo-knowledgebase` — nice, but
  it's the only command that prints a banner-ish first line; mildly breaks
  "text output is data" expectations when scripting in text mode.
- Feature I'd like: `hyalo find --property 'x!=v'` exists, but there's no
  `find --changed-since <ref>`; the `--files-from <(git diff …)` pattern works
  but a built-in would be friendlier. (Minor; the cookbook already teaches
  the pipe pattern.)
- Otherwise: hints are genuinely useful and every one I ran worked; the
  `set --validate` enum suggestion ("did you mean completed?") is excellent;
  toggle round-trip restored file state exactly (git diff clean after
  toggle/un-toggle).

## Honest overall assessment

**Strengths**: unusually high engineering discipline for a young tool — real
fuzzing, defense-in-depth on every untrusted-input path, execution-tested
hints, streaming/bounded memory behavior, fast (claims verified), excellent
error messages with self-healing hints, and a coherent JSON envelope contract
that agents can rely on. The security posture against hostile vault content is
better than most tools in this space.

**Weaknesses**: the top-level help has drifted from reality in the exact
places a newcomer reads first (F-1..F-4); a misleading error message on
invalid-UTF-8 reads (F-5); some very large single files in the CLI crate;
stale-index staleness is warn-but-serve (S-2); and the test suite, while
broad, has no mechanical guard tying help prose to the real CLI surface — the
one gap all the help bugs share.

**Would I recommend it?** Yes, with confidence — for both humans and agents.
For agents specifically it's close to ideal: structured output, dry-runs,
idempotent mutations, refusal-over-corruption semantics, and hints that teach
the next step. The defects found here are documentation-surface and one
diagnostic-message bug, not trust bugs: nothing observed could corrupt a
vault, escape a boundary, or hang a pipeline. Fix F-1..F-5 and the tool's
self-description will be as trustworthy as its behavior already is.
