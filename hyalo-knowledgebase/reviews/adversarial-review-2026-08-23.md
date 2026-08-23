---
title: Adversarial review 2026-08-23 — security deep-dive against untrusted vault threat model
type: review
date: 2026-08-23
tags:
  - review
  - security
  - audit
status: active
related:
  - "[[reviews/codebase-review-2026-08-06]]"
  - "[[reviews/deep-analysis-2-2026-08-23]]"
---

# Adversarial code review — hyalo 0.20.0 (2026-08-23)

Reviewer executed code: `cargo build --release` binary against scratch vaults under `/tmp`.
Labels: **VERIFIED** = reproduced with a command; **SUSPECTED** = static reasoning only.

No CRITICAL findings. The path-boundary layer (`fs_util.rs`, iter-202) held up under
every escape attempt I threw at it: `..` traversal, absolute link targets, symlinks to
files and directories outside the vault, `links fix --apply`, `mv`, `create-index -o`,
`set` — all refused or contained. The scanner's size caps (100 MB file / 1 MiB line /
64 KiB frontmatter / YAML anchor budget) also survived pathological inputs (billion-laughs
alias bomb, 3 MB frontmatter value, 80-deep nested brackets, CRLF, invalid UTF-8). Where
hyalo is weakest is exactly where the threat model says it should be strongest: **the
untrusted repo's own `.hyalo.toml` redefines the boundary the rest of the code defends.**

---

## HIGH

### H-1. `dir` in a project-local `.hyalo.toml` is unrestricted — an untrusted repo grants itself an arbitrary vault root (write scope escape)

**Location:** `crates/hyalo-cli/src/config.rs:706`

```rust
dir: cfg.dir.map(PathBuf::from).unwrap_or(defaults.dir),
```

The configured `dir` is taken verbatim. There is no check that it is at-or-below the
config directory; absolute paths and `..` are both accepted. Every downstream boundary
gate ("must be relative and within the vault", `resolve_file`, `atomic_write_within`)
then enforces containment against **this attacker-chosen root**.

**VERIFIED.** Repro (malicious repo cloned at `/tmp/evilrepo`, user `cd`s into it):

```console
$ mkdir -p /tmp/evilrepo/docs && cd /tmp/evilrepo/docs
$ printf 'dir = ".."\n' > .hyalo.toml
$ printf '%s\n' '---' 'title: x' '---' 'body' > a.md
$ hyalo mv docs/a.md stolen.md        # cwd is docs/, vault root is now /tmp/evilrepo
→ {"from":"docs/a.md","to":"stolen.md","total_files_updated":0,...}
$ ls /tmp/evilrepo
docs  secret.txt  stolen.md            # file moved OUT of the repo, into the parent dir
```

An absolute target is also accepted silently:

```console
$ printf 'dir = "/Users/james"\n' > .hyalo.toml
$ hyalo config | grep '"dir"'
  "dir": "/Users/james",
```

No warning is printed. The ancestor-adoption path *does* have a containment check
(`config.rs:426-429`, `canonical_cwd.starts_with(&vault)`), and `[changelog] path` is
validated against the config dir (verified: `../../../tmp/evil-out/CHANGELOG.md` refused
with *"resolves outside vault boundary"*), but the local-config `dir` path has neither.

**Impact:** run any mutating hyalo command inside a cloned malicious repo and its config
widens the write scope to any directory it names — the parent of the clone, or `$HOME`
via an absolute path. Because hyalo is designed to be driven by agents (CLAUDE.md
instructs agents to run hyalo commands verbatim from hints), a hostile repo plus a
normal agent loop is a plausible write-anywhere-within-scope primitive.

**Fix:** for a project-local (non-`--dir`, non-global) config, refuse `dir` values that
resolve above the config directory or that are absolute; at minimum, print a loud
warning when the resolved vault root is not at-or-below the config dir, mirroring the
`announce_ancestor_config` treatment. Note iter-201 deliberately moved in the opposite
direction ("no silent config discard") — the decision itself is what needs revisiting:
*honoring* the config is fine; *honoring its scope expansion* silently is not.

---

## MEDIUM

### M-1. A single invalid-UTF-8 file makes `hyalo lint` / `lint --fix` abort the entire run

**Location:** `crates/hyalo-cli/src/commands/lint.rs:2226`

```rust
std::fs::read_to_string(full_path).with_context(|| format!("reading {rel_path}"))?;
```

The `?` propagates out of the per-file loop and kills the whole command.

**VERIFIED:**

```console
$ printf '---\ntitle: bad\n---\n\xff\xfe invalid \xff\n' > invalid.md
$ hyalo lint >/dev/null 2>&1    → exit 2, whole run aborted
$ hyalo lint --fix >/dev/null   → exit 2
$ hyalo find / summary / links  → exit 0 (find even includes the file in results;
                                   links skips bomb.md with a warning)
```

One non-UTF-8 file anywhere in a 14k-file vault disables lint entirely — and `lint
--fix`, so a single corrupt file blocks all autofix workflows. Other commands already
have the right behavior (skip + warn, per `scanner/mod.rs:99-104` lossy handling).

**Impact:** availability — one untrusted/corrupt file turns a vault-wide gate into a
hard failure with no per-file skip.

**Fix:** catch the UTF-8 error per file, emit one diagnostic per offending file, and
continue; exit non-zero at the end if strictness requires it.

### M-2. Windows drive-relative paths and NTFS ADS are outside every boundary gate

**Location:** `crates/hyalo-core/src/index.rs:659-676` (SEC-1 path validation) and
`crates/hyalo-cli/src/config.rs`/`discovery.rs` `resolve_file` (per DEC entry,
decision-log.md:66: "rejects absolute paths, backslash-prefixed paths, and any path
containing `..` segments").

On Windows, `C:foo` is drive-relative: `Path::is_absolute()` returns `false` and it has
no `ParentDir` component, so the SEC-1 snapshot check and the lexical `resolve_file`
gate both pass it. At use time it resolves against the process CWD on drive `C:` —
potentially outside the vault. Similarly `a.md:stream` (NTFS alternate data stream)
is lexically inside the vault, so `set`/`append` would write an ADS instead of the
file (silent wrong-target write, no escape).

**SUSPECTED** — not executable on this macOS host. To confirm: on Windows, craft a
snapshot index with `rel_path = "C:\\Windows\\..."`-style drive-relative entries and a
vault file referenced as `note.md:ads`, then run `hyalo set`.

**Fix:** on Windows, reject any rel path containing a `Prefix` component *or* a
colon in the final component; the existing `Component::Prefix(_)` match in SEC-1 only
fires for absolute prefixed paths, not drive-relative ones.

---

## LOW

### L-1. `create-index -o` replaces a symlink instead of following it — inconsistent with the documented DEC-062 write policy

**Location:** `crates/hyalo-core/src/index.rs:925-929`

```rust
let mut tmp = NamedTempFile::new_in(parent)
    .with_context(|| format!("failed to create temp file for index"))?;
...
tmp.persist(path)
```

`write_snapshot` uses raw `NamedTempFile::new_in` + `persist` — it does not go through
`fs_util::write_impl`'s `resolve_write_target`, which is the documented policy
("when the destination is a symlink … the *target* is replaced; the symlink stays a
symlink (DEC-062)", `fs_util.rs:198-205`).

**VERIFIED:**

```console
$ ln -s /tmp/outside-target.md idx-link
$ hyalo create-index -o idx-link   → success JSON
$ ls -la idx-link                  → regular file (104 bytes), symlink gone
$ cat /tmp/outside-target.md       → untouched
```

Safety-neutral here (replacing the link cannot escape, and it dodges what would
otherwise be a symlink escape), but it silently diverges from the stated policy: a
user who symlinks `.hyalo-index` to a shared location gets the link clobbered with no
error. It also means the `-o` path gate and the frontmatter write path give different
answers for the same input.

**Fix:** route `write_snapshot` through `fs_util::atomic_write`/`atomic_write_within`
(or `resolve_write_target` + the existing boundary check) so all writes share one
symlink policy; or amend DEC-062 to say index writes replace links and document why.

---

## ADVISORY

- **Dependency advisories (VERIFIED via `cargo audit`):** `bincode` 1.3.3
  (RUSTSEC-2025-0141) and `yaml-rust` 0.4.5 (RUSTSEC-2024-0320), both via
  `comrak 0.21 → syntect 5.3`, are allowed in `deny.toml` with a re-verified rationale —
  reasonable. However `anyhow` RUSTSEC-2026-0190 (`Error::downcast_mut()` unsoundness)
  is *not* in the ignore list; if `cargo deny check` is a CI gate it may now fail, and
  if it doesn't, the advisory is silently untriaged. Add it to the ignore list with a
  rationale or bump anyhow.
- **YAML-bomb rejection leaks parser internals (VERIFIED):** the alias-bomb file
  produces `warning: skipping bomb.md: failed to parse YAML frontmatter: error: line 1
  column 7: budget breached: Anchors { anchors: 1 }` — the `Anchors { anchors: 1 }`
  debug struct is saphyr-internal jargon. Map it to a human message.
- **Case-insensitivity probing writes files into the user's vault**
  (`case_index.rs:420-437`, `CASE_PROBE_PREFIX` create/delete): transient probe files
  in the vault directory will ping file watchers and can race `git status`. Prefer a
  probe in a temp dir on the same filesystem.
- **Residual TOCTOU in `atomic_write_within` is documented** (`fs_util.rs:226-233`) —
  accepted-risk reasoning is sound for a single-user local CLI; no action.
- **Code-quality discipline is genuinely good** (one sentence, as instructed): every
  `unwrap()`/`expect()` I found outside `#[cfg(test)]` modules is test code; the two
  `unsafe` blocks (`broken_pipe.rs:47`, `index.rs:980`) are narrowly scoped, justified
  with SAFETY comments, and PID-range-checked; hints shell-quote their arguments and
  output is ANSI-sanitized (`output.rs:126`, verified with an OSC-escape file);
  e2e tests are behavior-asserting (checked `mv.rs`), not snapshot-diffing.

---

## Not reviewed

- `auto_link.rs` internals (3497 lines) — only exercised indirectly via `links`/`links
  fix` on adversarial files; no unit-level audit of its scoring/rewriting logic.
- `bm25.rs` ranking math and persisted-inverted-index consistency beyond `validate_doc_ids`.
- `hyalo-mdlint` rule semantics and the `mdbook-lint` dependency's parser (`comrak`)
  as an untrusted-input surface.
- `views`, `filename_template`, `okf`/`madr`/`changelog` generators beyond the
  changelog path-traversal probe.
- Real Windows behavior (see M-2), real case-insensitive-filesystem behavior beyond
  `case_index` unit tests.
- Concurrency stress (two simultaneous mutating hyalo processes on the same vault);
  only the single-writer atomicity machinery was reviewed statically.
- Performance at true 14k-file scale (only `bench-e2e.sh` existence noted); link
  fuzzy-candidate perf is a known open item (iter-206).
- The ~4,800-test suite's coverage quality beyond spot checks; `xtask` CI gates.
