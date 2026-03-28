---
branch: iter-54/preserve-frontmatter-formatting
date: 2026-03-27
status: completed
tags:
- ux
- parser
title: Preserve frontmatter formatting on write
type: iteration
---

# Preserve frontmatter formatting on write

## Motivation

When hyalo mutates frontmatter (via `set`, `remove`, `append`, etc.), the YAML is deserialized
into a `BTreeMap<String, Value>` and then re-serialized with `serde_saphyr::to_string`. This
causes two kinds of unwanted reformatting:

1. **Key reordering** — `BTreeMap` sorts keys alphabetically, destroying the author's original
   key order (e.g. `title` before `date` becomes `branch`, `date`, `status`, `tags`, `title`).
2. **List indentation change** — serde-saphyr's default `compact_list_indent: true` produces
   flush `- item` instead of the common hand-written `  - item` style.

This is a papercut that shows up in every `git diff` after a hyalo mutation and makes diffs
noisy. It also discourages adoption — users don't want a tool that silently reformats their files.

See: [[iteration-53-saphyr-hardening]] for the serde-saphyr migration that makes this fix possible.

## Approach

serde-saphyr provides the tools we need:

- **`IndexMap`** (from the `indexmap` crate) preserves insertion order. Switching `Document`'s
  `properties` from `BTreeMap<String, Value>` to `IndexMap<String, Value>` preserves the original
  key order through a deserialize → mutate → serialize roundtrip.
- **`SerializerOptions`** with `compact_list_indent` controls the list indentation style.
  `to_string_with_options()` accepts `SerializerOptions` for serialization control.
- **Per-document style detection** — before parsing, scan the raw YAML for the first sequence
  item (`- `) and check whether it's indented relative to its parent mapping key. Store this as
  a `compact_list_indent` flag on the `Document` so that re-serialization preserves the original
  style. This avoids forcing a global default that reformats half the user's files.

  Detection heuristic: find the first occurrence of a mapping key followed by a newline and a
  sequence indicator (`- `). If the `- ` is indented further than the key, it's non-compact
  (`compact_list_indent: false`). If flush or at the same level, it's compact (`true`). If no
  sequences exist, default to non-compact (the common hand-written convention).

## Tasks

### Switch to IndexMap for key-order preservation
- [x] Add `indexmap` as a workspace dependency
- [x] Replace `BTreeMap<String, Value>` with `IndexMap<String, Value>` in `Document` and all public APIs in `frontmatter.rs`
- [x] Update `read_frontmatter`, `read_frontmatter_from_reader`, and `Document::parse` to deserialize into `IndexMap`
- [x] Update all consumers in hyalo-core and hyalo-cli that use `BTreeMap<String, Value>` for properties (scanner, index, filter, commands)
- [x] Verify that `serde_saphyr::from_str_with_options` deserializes into `IndexMap` preserving key order
- [x] Unit test: roundtrip preserves original key order

### Detect and preserve list indentation style
- [x] Add a `compact_list_indent: bool` field to `Document` (default: `false`)
- [x] Implement `detect_list_indent_style(yaml: &str) -> bool` that scans raw YAML for the first sequence item indentation relative to its parent key
- [x] Call the detector in `Document::parse` and `read_frontmatter_from_reader` before deserializing, store the result on the `Document`
- [x] Create `hyalo_serializer_options(compact_list_indent: bool) -> SerializerOptions` helper
- [x] Replace all `serde_saphyr::to_string()` calls with `serde_saphyr::to_string_with_options()` using the per-document options
- [x] `write_frontmatter` needs the style flag — either accept it as a parameter or detect it from the existing file before overwriting
- [x] Unit test: detect compact style (`- item` flush with key)
- [x] Unit test: detect indented style (`  - item` under key)
- [x] Unit test: no sequences defaults to non-compact
- [x] Unit test: roundtrip of compact-style frontmatter preserves compact style
- [x] Unit test: roundtrip of indented-style frontmatter preserves indented style

### Verify no regressions
- [x] Existing tests pass with IndexMap (serde_json::Value roundtrip, property operations, etc.)
- [x] E2E: `hyalo set` on a file, then `git diff` shows only the changed property — no reformatting noise
- [x] Dogfood: run `hyalo set --property status=completed --file iterations/iteration-54-preserve-frontmatter-formatting.md` and verify minimal diff

### Quality gates
- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`

## Risk

- **IndexMap is a new dependency** — but it's a widely used, well-maintained crate (~200M downloads) with no transitive surprises. It's already an indirect dependency via serde_json.
- **API surface change** — `BTreeMap` → `IndexMap` in public types is a breaking change for any downstream consumers. Since hyalo has no external API consumers, this is safe.
- **Sorted output in tests** — some tests may rely on alphabetical key order in serialized output. These will need updating.
- **JSON output** — `serde_json::Value`'s `Object` variant uses `serde_json::Map` which is backed by `BTreeMap` (or `IndexMap` with the `preserve_order` feature). Check whether the JSON output commands are affected.
