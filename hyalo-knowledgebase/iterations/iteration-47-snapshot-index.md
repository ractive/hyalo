---
branch: iter-47/snapshot-index
date: 2026-03-26
status: in-progress
tags:
- performance
- index
- cli
- architecture
title: Iteration 47 — Snapshot Index for Repeated Queries
type: iteration
---

# Iteration 47 — Snapshot Index for Repeated Queries

## Motivation

Skills like `hyalo-dream` run many sequential queries against the same vault. Each invocation
rescans all files to build the in-memory index. On a 3,500-file vault this is ~500ms per call,
compounding to 15+ seconds over 30 queries. A snapshot index lets the vault be scanned once
and reused across all subsequent queries in a session.

## Design Decisions

- **Core abstraction**: `VaultIndex` trait decouples commands from their data source.
  Commands receive `&dyn VaultIndex` and don't know whether data came from a filesystem
  scan or a serialized file. This enables future backends (SQLite, etc.) without changing
  command code.
- **First backend**: bincode snapshot file (`.hyalo-index`) — binary, opaque, not an API
- **Scope**: read-only commands only (`find`, `summary`, `tags`, `properties`, `backlinks`);
  mutation commands (`set`, `remove`, `append`, `mv`) always scan from disk
- **Content search**: always scans disk regardless of `--index` — it's query-dependent
  and can't be pre-indexed. No warning needed, no special handling
- **No partial indexes**: full vault snapshot every time (rebuild is <500ms)
- **No backwards compatibility**: if bincode deserialization fails (e.g. after a hyalo upgrade),
  warn and fall back to normal scanning
- **No daemon / file watcher**: deferred to a future iteration if needed
- **Stale detection**: PID in index header; `create-index` checks for existing `.hyalo-index`
  files with dead PIDs and warns

## Architecture

### The VaultIndex Trait

```rust
/// Abstraction over how vault data is obtained.
/// Commands program against this trait, not a concrete data source.
trait VaultIndex {
    fn entries(&self) -> &[IndexEntry];
    fn get(&self, rel_path: &str) -> Option<&IndexEntry>;
    fn link_graph(&self) -> &LinkGraph;
}
```

### IndexEntry — Per-File Data

The intermediate data between scan and filter/format:

- `rel_path: String` — relative path within vault
- `modified: String` — ISO 8601 mtime
- `properties: BTreeMap<String, serde_yaml_ng::Value>` — raw frontmatter (for filtering)
- `tags: Vec<String>` — extracted from properties
- `sections: Vec<OutlineSection>` — heading outline
- `tasks: Vec<FindTaskInfo>` — checkbox items
- `links: Vec<(usize, Link)>` — outbound links with line numbers

### Implementations

**`ScannedIndex`** — extracts the current inline scan logic from commands into a reusable
builder. This is not new functionality — it's a refactor of what each command does today
into a shared struct behind the `VaultIndex` trait.

**`SnapshotIndex`** — deserializes a `.hyalo-index` file (bincode). Contains the same
`IndexEntry` data plus a header with metadata:

- `vault_dir: String` — canonical vault path (validated on load)
- `site_prefix: Option<String>` — how links were resolved (must match current config)
- `created_at: u64` — unix timestamp
- `pid: u32` — creator PID (for orphan detection)

### Flow

```
main() decides based on --index flag:
  ├─ --index given  → SnapshotIndex::load(path)?  → &dyn VaultIndex
  └─ no --index     → ScannedIndex::build(dir)?    → &dyn VaultIndex
                                                         │
                              ┌───────────────────────────┘
                              ▼
                    command(index: &dyn VaultIndex, ...)
                       │
                       ├─ index.entries() / index.get() for per-file data
                       ├─ index.link_graph() for backlinks
                       └─ disk scan only for content search (query-dependent)
```

## Tasks

### Phase 1: Abstraction (the main deliverable)
- [ ] Define `IndexEntry` struct in `hyalo-core` (new module `index.rs`)
- [ ] Define `VaultIndex` trait with `entries()`, `get()`, `link_graph()`
- [ ] Implement `ScannedIndex` — extract current per-file scan logic from `find`/`summary`/etc. into a shared builder behind the trait
- [ ] Add `Serialize`/`Deserialize` derives to: `IndexEntry`, `BacklinkEntry`, `Link`, `LinkKind`, `LinkGraph`, `OutlineSection`, `FindTaskInfo`
- [ ] Refactor `find` command to use `&dyn VaultIndex` instead of inline scanning; content search stays as disk I/O
- [ ] Refactor `summary`, `tags summary`, `properties summary`, `backlinks` to use `&dyn VaultIndex`

### Phase 2: Snapshot Backend
- [ ] Add `bincode` dependency to `hyalo-core/Cargo.toml`
- [ ] Implement `SnapshotIndex` — `load(path)` deserializes bincode, validates header, returns `impl VaultIndex`; graceful fallback to `ScannedIndex` on deserialization error
- [ ] Implement `SnapshotIndex::save(index: &dyn VaultIndex, path)` — serializes to bincode with header
- [ ] Add `create-index` subcommand: builds `ScannedIndex`, saves as snapshot, detects stale orphan `.hyalo-index` files
- [ ] Add `drop-index` subcommand: deletes index file, warns about other stale `.hyalo-index` files
- [ ] Add `--index <PATH>` global CLI flag (clap `global = true`)
- [ ] Wire up in `main()`: `--index` → `SnapshotIndex::load()`, else `ScannedIndex::build()`

### Phase 3: Tests & Validation
- [ ] Add e2e tests: create-index → find --index, drop-index, stale detection, incompatible index fallback, content search with --index
- [ ] Add benchmark: repeated `find --index` vs repeated `find` (measure cumulative savings)

## Future (deferred)

- RAII long-running `index-server` mode with file watcher and atomic rename
- stdin/stdout query protocol for tighter skill integration
- `--index=auto` with env var for transparent session-scoped caching
- SQLite backend behind `VaultIndex` trait
