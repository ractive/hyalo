---
title: "Iteration 310: move the YAML parser to serde-saphyr 1.x"
type: iteration
date: 2026-10-04
tags: [iteration, dependencies, yaml, frontmatter]
status: completed
branch: iter-310/serde-saphyr-1x
---

# Iteration 310: move the YAML parser to serde-saphyr 1.x

The dependency refresh (PR #361) held `serde-saphyr` at 0.0.23 because 1.3.0
changes how frontmatter parses. This iteration moves to 1.3.0 and keeps every
frontmatter value users already have. hyalo now resolves leading-zero integers
itself (DEC-350).

## Tasks

- [x] Research how 1.3 resolves plain scalars, its options and the upstream tracker
- [x] Compare the alternatives (own resolution, saphyr-parser, serde_norway, yaml-rust2, staying on 0.0.23) on the repo vault, MDN and two Obsidian vaults
- [x] Bump to 1.3.0 and set every new `Options`/`Budget` field deliberately
- [x] Resolve leading-zero plain scalars as integers in one unit-tested function
- [x] Keep `.nan`/`.inf` as the strings they were
- [x] Keep long comment runs parsable
- [x] Keep serializer output byte-identical (`a#b` quoting)
- [x] Tests for every neutralised or accepted 1.x change
- [x] Measure parse time before and after
- [x] Byte-identical `summary`/`properties`/`types list`/`find --fields properties` JSON on the vault and MDN
- [x] CHANGELOG, DEC-350, Cargo.toml comment, dependabot ignore removed, `cargo deny check`

## Acceptance criteria

- [x] The four 0.0.23 pinning tests in `frontmatter::tests` pass unchanged
- [x] `hyalo lint --strict` on the whole vault exits 0
- [x] Typed queries return the same counts before and after
- [x] All gates green on Linux, macOS and Windows

## Research

### How 1.3 resolves a plain scalar

`Deserializer::deserialize_any` (the typeless path `serde_json::Value` takes)
tries, in order: bool, integer, float, string. The integer step calls
`parse_int_unsigned` / `parse_int_signed` (`src/parse_scalars.rs`), which
reject a decimal with a redundant leading zero:

```rust
// Yaml 1.2 forbids decimal integer literals starting with zero.
if digits.starts_with('0') && digits != "0" { return Err(invalid()); }
```

The scalar then matches the YAML 1.2 float pattern, so `01234` becomes
`1234.0`. This is deliberate, and it rests on a misreading. The YAML 1.2
**JSON** schema forbids leading zeros. The **core** schema, which YAML 1.2
recommends and Obsidian follows, resolves `[-+]?[0-9]+` as an integer, and
its integer pattern is checked before its float pattern. The only related
option, `legacy_octal_numbers`, reads `0127` as base 8, which is YAML 1.1.
No option, scalar-resolver callback or tag-resolution hook exists.

What 1.3 does expose is `Spanned<T>`. Its `Location` carries a byte span
into the source when the input is a `&str`. A node type of hyalo's own that
wraps each child in `Spanned` can therefore read a float's source text back
and re-resolve it.

Upstream tracker (bourumir-wyngs/serde-saphyr, 83 issues and PRs): nothing
about leading-zero integers. The related issues are #151 (opt-in rejection of
non-finite floats, which shipped as the default-on
`reject_non_finite_typeless_float`) and #172 (exposing the comment lookahead
limit, which shipped as `Budget::max_buffered_comment_events`).

### Other 1.3 changes measured on real vaults

The repo vault (515 blocks), Obsidian Hub (6 509), kepano-obsidian (98) and MDN
`files/en-us` (14 375) were compared value by value against 0.0.23. With the
options below, 1.3 differs on **4 MDN files only**. 0.0.23 resolved the plain
words `NaN` and `Infinity` (Rust's `f64` spellings, not YAML's) as non-finite
floats and stored them as `".nan"` / `".inf"`. So `title: NaN` on MDN's NaN
page was reported as `".nan"`. 1.3 keeps them as the strings `"NaN"` /
`"Infinity"`, as the core schema does: only `.nan`, `.NaN`, `.NAN` and the
`.inf` family are special. This change fixes corrupted values, so it ships.

### New fields since 0.0.23 and the value chosen

| Field | 1.3 default | hyalo | Why |
|---|---|---|---|
| `Options::emit_comments` | `true` | `false` | hyalo never reads comment text. Off, comments are still validated but not buffered, so `max_buffered_comment_events` (default 32) no longer rejects a list under 33+ comment lines and comments stay out of `max_events`, which matches 0.0.23 |
| `Options::merge_keys` | `Merge` | `Merge` (default) | Same as 0.0.23: an inline `<<: {a: 1, b: 2}` expands identically under both versions. An aliased merge is already refused by `max_aliases: 0` |
| `Options::reject_non_finite_typeless_float` | `true` | `false` | Keeps `.nan`/`.inf`/`1e999` as the strings `.nan`/`.inf`/`-.inf`, as before |
| `Options::reject_unsupported_tags` | `false` | `false` (default) | Unknown tags were tolerated before too |
| `Options::property_syntax` | `Braced` | default | Only active with the `properties` feature, which is off |
| `Budget::max_buffered_comment_events` | 32 | default | Ineffective once `emit_comments` is off |
| `Budget::max_total_comment_bytes` | 64 MiB | default | Not enforced with `emit_comments` off. The 64 KiB frontmatter cap bounds comments anyway |
| `Budget::simple_key_max_lookahead` | 1 024 | default | The YAML spec's own simple-key limit |
| `Budget::flow_nesting_limit` | 255 | default | `max_depth: 20` is the tighter limit |
| `Budget::max_recorded_anchor_events` / `_bytes` | 1 M / 64 MiB | default | Unreachable with `max_anchors: 0` |
| `Budget::max_property_expansion_depth` / `max_total_property_interpolation_work` | — | default | `properties` feature off |
| `SerializerOptions::comment_position` | `Inline` | default | Only used for `Commented<T>`, which hyalo does not use |

The seven limits hyalo sets (`max_events` 10 000, `max_depth` 20,
`max_aliases` 0, `max_anchors` 0, `max_nodes` 5 000, `max_total_scalar_bytes`
64 KiB, `max_documents` 1) are unchanged. `Options`, `Budget` and
`SerializerOptions` are now `#[non_exhaustive]`, so hyalo assigns fields on a
`default()` value instead of using a struct literal.

### Alternatives

Parse time is for the frontmatter blocks only, measured with a scratch crate
(release build, best of three, same blocks for every candidate). "Diffs" is the
number of blocks whose parsed value differs from 0.0.23.

| Candidate | Maintenance | 1.2 core scalars | Positions (HYALO005) | Budgets | Duplicate keys | Serializer | unsafe | MDN parse | Obsidian Hub parse | Diffs |
|---|---|---|---|---|---|---|---|---|---|---|
| (a) serde-saphyr 1.3 + hyalo leading-zero resolution | 1.3.0, 2026-09-16, active | yes, once hyalo resolves leading zeros | line:col, same error variants | all seven, unchanged | yes | same emitter, run in `yaml_12` mode with a small quoting adapter | `#![forbid(unsafe_code)]` | 86.6 ms | 30.7 ms | 4 (the `NaN` fix) |
| (a') the same, always through the `Spanned` node | — | yes | yes | yes | yes | yes | none | 183.6 ms | 66.7 ms | 4 |
| (b) `saphyr-parser` / `granit-parser` events + own tree | 0.1.0 2026-09-19 / 1.3.0 2026-09-16 | hyalo would own all of it | marks only, hyalo writes every message | hyalo re-implements all seven | hyalo re-implements | none, so the serde-saphyr emitter stays anyway | none | not built: needs several hundred lines (budgets, duplicate keys, tags, error text) before it could be measured fairly | — | — |
| (c) `serde_norway` 0.9.42 | last release 2024-12-21 | YAML 1.1-ish resolver | line:col | recursion limit only | yes | different emitter, so the splice and round-trip contract would need re-pinning | wraps `unsafe-libyaml-norway` (C translated to Rust) | 74.3 ms | 20.2 ms | 4 |
| (d) `yaml-rust2` 0.11 + own conversion | 0.13.0, 2026-09-11, active | yes | line:col | none | silently last-wins | none | none | 65.7 ms | 19.2 ms | 4 MDN + 28 kepano templates it accepts where every other parser rejects them |
| (e) stay on serde-saphyr 0.0.23 | unmaintained pin, dependabot ignore | yes, but `NaN`→`".nan"` corruption | yes | yes | yes | yes | none | 78.5 ms | 26.4 ms | 0 |

Choice: **(a)**. It keeps the error variants hyalo maps (`friendly_parse_error`),
the seven budgets and the emitter that the splice engine and DEC-293's
round-trip contract are pinned against. Its own code is one small node type
plus one resolution function. That code runs only for a block whose fast
parse produced an integral float (`01234` → `1234.0`), which is rare: no
block in the four corpora goes through it. The per-file cost is a
float-presence walk over the parsed value. (b) would move every budget and
message into hyalo for no gain. (c) and (d) are faster but lose budgets or
duplicate-key errors, and (c) brings `unsafe` C-port code and a different
emitter. Recorded as DEC-350.

### Serializer

The 1.3 emitter, at its default `yaml_12: false`, quotes YAML 1.1 timestamps
and sexagesimals. So `hyalo set date=2026-01-01` would start writing
`date: "2026-01-01"`, where Obsidian and 0.0.23 write it plain. It also
writes `a#b` plain. `yaml_12: true` removes the timestamp quoting, but it
also stops quoting `yes`/`no`/`on`/`off`/`y`/`n`, and it prepends
`%YAML 1.2\n---\n`. hyalo therefore runs the emitter in `yaml_12` mode,
strips that header, and wraps each single-line string that 0.0.23 quoted
and 1.3 would write plain in `serde_saphyr::DoubleQuoted`
(`keeps_legacy_quotes`: YAML 1.1 booleans, an inner `#` that does not start
a comment, underscored digit runs). Under `quote_all` nothing is wrapped.

A differential harness serialized 58 023 distinct strings through 0.0.23 and
through hyalo's `emit_map`, in value, list-item and key position under the
four option sets hyalo uses. The strings were every key and string value in
the four corpora plus synthetic edge cases. It also serialized 21 469 whole
frontmatter maps. All 21 469 maps are byte-identical, and every emitted
string reads back unchanged under 1.3. The remaining string differences are
accepted, because each one quotes a value 0.0.23 wrote in a form that does
not read back as the same string or is ambiguous: `---`, `...`, `--- x`,
`0X1F`, a trailing space, and `<<` as a key. Plus one cosmetic change: the
keys `yes`/`no`/`on`/`off`/`y`/`n` and keys with an inner `#` are written
unquoted, because serde-saphyr's key path ignores the `DoubleQuoted`
wrapper. They read back as the same strings.

### Results

- Before/after JSON on this vault: `summary`, `properties`, `types list`,
  `lint --strict`, `find --fields properties,tags --limit 0`,
  `find --property 'date>=2026-01-01' --count` (515 → 515) and
  `find --property status=completed --count` are byte-identical. They are
  also identical for the Obsidian Hub `find`, and for MDN `summary` and
  `properties`.
- MDN `find --fields properties,tags --limit 0`: exactly 4 of 14 375 files
  differ. `title: NaN` and `title: Infinity` now read as `"NaN"` and
  `"Infinity"` instead of `".nan"` and `".inf"`.
- `hyalo set` output for 35 tricky values plus three `append`s is
  byte-identical before and after.
- Timing (hyperfine, mean of 5, release): MDN `summary` 1.043 s → 1.033 s;
  MDN `find --fields properties --limit 0` 524 ms → 493 ms (user
  450 → 476 ms); vault `summary` 54.1 ms → 53.6 ms (mean of 10).
