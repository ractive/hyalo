---
type: review
title: Codebase review — 12 September 2026
date: 2026-09-12
status: active
tags:
  - review
  - architecture
  - security
---

# Codebase review — 12 September 2026

Reviewed `36745a4604d8c398303cee61027c7151c778996a` on macOS using a fresh release
build of v0.23.0. This is a new review, not an assertion that the previous
finding ledger has been exhaustively reverified. Baselines:
[[reviews/astra-code-review-2026-09-09]] and
[[research/rust-architecture-review-2026-09-10]]. Runtime details:
[[dogfood-results/dogfood-v0230-2026-09-12]].

The architecture is substantially more coherent now. Keep the current crate
split and the new checked boundaries. However, there are still two reproducible
ways to delete user-owned content and three material correctness/usability
problems. They are concentrated in consumers that bypass a shared semantic
policy, rather than in the new filesystem publication machinery.

## Findings

P1/high means fix before trusting the affected write workflow. P2/medium means
a material correctness or supported-workflow problem. All five groups below
were reproduced with the reviewed release binary; none requires fuzzing,
resource exhaustion, concurrency injection, or a hostile remote service.

### F1 — P1/high: MADR generation deletes prose after a literal marker example

**Location:** `crates/hyalo-cli/src/commands/managed_region.rs:62`, used by
`crates/hyalo-cli/src/commands/madr.rs:127`.

`Markers::splice` finds the first opening-marker substring anywhere in the
file, then the next closing-marker substring. It does not distinguish an actual
managed delimiter from a literal example. With this ordinary README:

```markdown
# Decisions

Example marker: `<!-- madr:toc:begin -->`

DO NOT DELETE THIS USER TEXT

<!-- madr:toc:begin -->

Old generated table

<!-- madr:toc:end -->

USER FOOTER
```

Place it at `kb/docs/decisions/README.md`, add an ADR at
`kb/docs/decisions/0001-choice.md`, and use `dir = "kb"`. Running
`hyalo madr toc --apply` exits 0 and deletes the handwritten paragraph, the real
opening delimiter, and the closing backtick of the example. The footer remains.
No `--replace` was supplied. The preview reports an update but does not expose
which user prose is about to disappear.

**Fix:** Recognize unique, standalone managed markers outside literal syntax;
reject duplicate, reversed, or incomplete pairs before publication. Share the
marker classification with `has_markers`. The current regression only tests a
literal *end* marker before the real start, which does not cover this failure.
Check preservation of every byte outside the real managed span.

Evidence: `madr-marker-preview-correct.json`,
`madr-marker-apply-correct.json`, and the `madr-markers-correct` fixture in the
local evidence directory.

### F2 — P1/high: Pi setup takes ownership of an unrelated package manifest

**Locations:** `crates/hyalo-cli/src/commands/init.rs:929` and
`crates/hyalo-cli/src/commands/init.rs:1454`.

Start with an existing `.pi/package.json`:

```json
{"private":true,"dependencies":{"custom-extension":"1.0.0"}}
```

`hyalo init --pi` exits 0 and replaces the complete manifest with Hyalo's bundled
manifest, deleting the dependency and the other existing fields. Its report
calls this an update. Separately, `hyalo deinit` deletes this same user-authored
manifest even in a project that never installed Hyalo's Pi integration. Both
cases were reproduced independently.

This is actual configuration loss, with a plausible consequence of breaking
other project-local extensions. Path confinement is working here; the missing
boundary is **artifact ownership**. Being inside the installation root is not
proof that a shared file belongs to Hyalo.

**Fix:** Merge only the fields Hyalo owns, preserve dependencies and unrelated
settings, and retain enough ownership information for removal. An unrecognized
existing manifest must be preserved or cause a clear refusal. Apply the same
policy to init and deinit; symlink checks alone do not address this bug.

Evidence: `pi-init.json`, `pi-deinit-unmanaged.json`; fixtures `pi-existing` and
`pi-deinit-unmanaged`.

### F3 — P2/medium: successful moves generate links that no longer resolve

**Location:** `crates/hyalo-core/src/link_write.rs:124`; target computation at
`link_write.rs:263` supplies raw destination characters to the splice.

Create `note.md` and a referring note containing:

```markdown
[Note](note.md)

[[note]]
```

Run `hyalo mv note.md 'C#.md'`. It exits 0, reports both links updated, and writes
`[Note](C#.md)` and `[[C#]]`. Hyalo subsequently interprets the target as `C`,
with `.md` treated as the Markdown fragment; both links have `path: null`.
`hyalo find --broken-links --count` reports one affected file.

The separate ordinary filename `Release (final).md` also breaks the Markdown
link after a successful move: Hyalo reads its target as `Release (final`, while
the rewritten wikilink resolves. These are independent copies of the fixture,
not effects of an earlier failed move.

**Fix:** Emit destinations using the rules of each link syntax, including URL
encoding or supported escaping. If a wikilink cannot represent a destination
safely, refuse or report the unsupported rewrite before moving. Validate that
the rendered occurrence resolves to the planned destination, rather than only
validating that the resulting file is parseable Markdown/frontmatter.

Evidence: `linkname-hash-move.json`, `linkname-hash-links.json`,
`linkname-paren-move.json`, `linkname-paren-links.json`.

### F4 — P2/medium: the scaffold emitter changes defaults or emits unreadable YAML

**Locations:** `crates/hyalo-cli/src/commands/new.rs:537` and
`crates/hyalo-cli/src/commands/new.rs:637`; preflight at `new.rs:108` checks only
frontmatter byte/line size.

With the following valid configuration:

```toml
dir = "kb"
[schema.types.note]
required = ["title"]
[schema.types.note.defaults]
title = "[Draft]"
[schema.types.note.properties.title]
type = "string"
```

`hyalo new --type note --file note.md` exits 0 and emits `title: [Draft]`.
`hyalo find --file note.md --fields properties` returns an array, and lint
rejects the declared string property. A `"{draft}"` default similarly becomes
a mapping. Replacing the default with the TOML string `"first\nsecond"` emits a
literal newline inside a minimally escaped double-quoted scalar; the normal
reader rejects the generated frontmatter and named `find` exits 1.

These are supplied values being misserialized, not the intentionally invalid
`TBD` placeholders documented for `new`.

**Fix:** Route scaffold keys and string values through the shared YAML emitter,
then validate the complete document with the normal reader before creation.
Assert exact value/type round trips for bracketed, brace-containing, and
multiline defaults. Quoting keys also needs the shared policy.

Evidence: `default-list-*`, `default-brace-*`, `default-newline-*` result files
and their small fixtures.

### F5 — P2/medium: fresh MDN snapshots immediately lose ranked-search acceleration

**Location:** `crates/hyalo-core/src/index.rs:694`; the full expansion estimate is
computed at `crates/hyalo-core/src/bm25.rs:934`.

On the local 14,375-file MDN checkout:

```sh
hyalo create-index --dir /path/to/mdn/files/en-us \
  --output /tmp/mdn.index --allow-outside-vault --no-hints
hyalo find AbortController --dir /path/to/mdn/files/en-us \
  --index-file /tmp/mdn.index --limit 5 --no-hints
```

Creation succeeds with `warnings: 0`. The very first indexed ranked query emits
`BM25 expanded-token budget exceeded; rebuild the snapshot` and falls back to
live reads. It took 3.856 s versus 3.939 s for the disk query in these individual
samples. Results were equal. Metadata indexing remained useful: 0.292 s versus
0.591 s for the tested title filter.

The safety cap is appropriate; the problem is applying the budget for expanding
*every* token to admission of a compact index that can score directly from
postings. Creation and consumption also disagree about whether the artifact is
usable, and rebuilding the same content cannot remedy a content-based cap.

**Fix:** Keep bounded reconstruction and malformed-index rejection, but separate
them from direct-postings scoring eligibility. At minimum, surface the actual
limitation at creation/load and stop recommending an ineffective rebuild. Use
an ordinary large corpus alongside the small deterministic scale fixture.
Do not remove the cap merely to make this check green.

Evidence: `mdn-index.json`, `mdn-find-index.json`, `mdn-find.json`, and the two
`mdn-metadata-*` outputs. Timings are spot-checks, not a controlled benchmark or
a claimed factor-of-two regression against the previous release.

## Architecture assessment

### What now makes sense

- **The existing crate split is appropriate.** Core owns documents, querying,
  links, and checked filesystem primitives; mdlint owns validation/fix proposals;
  the CLI coordinates applications and rendering. npm/Pi remain process adapters.
  A new crate split or a wholesale rewrite would not fix the findings above.
- **Prepared invocation is a meaningful boundary.** Output preflight now runs
  before mutations; explicit selection/cardinality is represented by prepared
  types. Invalid jq, invalid task output, and empty-selection checks behaved
  correctly in this round.
- **Filesystem and mutation ownership are materially better.** Captured inputs
  retain exact bytes and identity; prepared changes precede publication;
  `WriteSession` makes finalization explicit; the apply coordinator keeps effects
  and reconciles the snapshot even after failures. Static external symlinks and
  filesystem-equivalent move destinations were refused in the exercised cases.
- **Shared syntax and catalog ownership pay off.** Literal task examples survived
  fixes. Boolean/phrase search regressions were fixed. Indexed and disk file
  objects were equal after set, task toggle, and a backlink-rewriting move.
  Complete index replacement plus explicit batch finishing is a better invariant
  than callers updating selected fields.
- **The output path no longer serializes a JSON string only to parse it again.**
  `CommandOutcome::Success` carries a JSON value and separate effects/status.
  The npm/Pi regressions exercise actual observed mutation reports and diagnostics.

### Where the migration remains incomplete

The five findings expose incomplete semantic ownership: a private scaffold YAML
emitter, substring-based MADR markers, unowned shared installation artifacts,
raw link-target emission, and an expansion budget attached to the wrong index
operation. Atomic replacement faithfully publishes these incorrect plans; it
cannot make the plans correct. Finish these consumers against shared parsing,
encoding, and ownership rules.

`PreparedCommand::Legacy`, the broad mutable `CommandContext`, and generic JSON
values still leave some contracts enforced by convention. They are manageable
migration debt; prioritize migrating a family when an actual invariant requires
it rather than adding another universal abstraction. Likewise, core still has
process-global `OnceLock` scan/alias configuration (`discovery.rs:25`, `:42`,
`:243`). That is workable for the one-invocation CLI, but should become explicit
per-session policy before treating core as a multi-vault service library.

Several readers still use checked paths plus a later raw open rather than the
`OpenedTarget` handle, for example the ranked fallback in `find/mod.rs:676` and
`read.rs:229`. This round's static escape checks passed. The source does not
establish protection against concurrent directory replacement, and the rooted
module explicitly disclaims that guarantee. This is a documented limit, not an
additional reproduced exploit in this review.

**Assessment:** The architecture now makes sense for the CLI. Its important
abstractions are doing useful work. I would fix the two content-loss cases and
the link/scaffold semantics before broad unattended maintenance; I would not
restart the architecture project.

## Security and validation

No new external-root read/write escape or code-execution issue was reproduced.
Named reads, indexed regex/body search, set, move, init, and deinit refused the
static external symlinks tested. External sentinel bytes remained unchanged.
An alias-to-referent move and an applied case-equivalent batch move were refused
without losing the source notes. The Pi ownership issue is a local integrity
problem, not a remote execution finding.

- Formatting check and strict workspace Clippy passed.
- `cargo test --workspace -q`: 5,153 passed, zero failed, two ignored doctests.
- npm package suite: 38 passed, including native-binary API and Pi tests.
- `cargo audit`: zero vulnerabilities and no warning entries against 1,243
  advisories; database commit `b50980aad8b8f14f77e25a97b32dd94bf008b0af`, updated
  9 September 2026. npm audit: zero vulnerabilities.
- A redaction-safe pattern screen of 1,028 tracked text files found no matching
  private keys, GitHub tokens, AWS access-key IDs, Stripe live keys, or credential
  URLs. This was a bounded pattern screen, not a complete historical secret audit.
- The dogfood evidence contains 92 recorded CLI invocations before report
  authoring, including exploratory negative controls; no command timed out.
  This count is not an exhaustive coverage claim.

The review was performed locally in one context. No new fuzz campaign, allocation
attack, race harness, native Windows/Linux run, live LLM/Pi session, or complete
release/supply-chain audit was performed. Existing automated tests supplied
additional regression coverage; they do not negate the runtime findings.

The evidence directory is `/tmp/hyalo-review-20260912`. The tested executable is
`bin/hyalo` there, built from the clean reviewed tree with its workspace lockfile;
SHA-256 `2f9faa40f52c76c085abe0b88c5c6385062d0a5dbfe04ff875b9ce367446f39e`.
All counted product reproductions used that fixed artifact. Initial discovery
found stale PATH installations, which were excluded from product evidence. The
Cargo-installed executable was refreshed to the tested artifact; a Homebrew
installation can still take precedence in another shell.

Only this report and the companion dogfood report were added to the repository.
Product source is unchanged; no findings were fixed, committed, pushed, or merged.

**5 findings: 0 critical, 2 high, 3 medium, 0 low**

The repaired confinement and mutation machinery materially improves local data
safety. The remaining highest-priority gaps are ownership of existing user data
and correctness of the planned document changes, rather than a demonstrated
remote attack surface.
