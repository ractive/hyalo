---
title: "Iteration 94: Views (Saved Find Queries)"
type: iteration
date: 2026-04-02
tags:
  - iteration
  - cli
  - find
  - views
status: in-progress
branch: iter-94/views
---

## Goal

Add named, reusable find queries ("views") stored in `.hyalo.toml`. Users save a combination of find filters under a name and recall them with `--view <name>`. Discoverable via `hyalo views list`.

Benefits: fewer tokens for LLMs, fewer typos, project-specific vocabulary, runtime discoverability.

## Commands

```
hyalo find --view <name> [additional filters...]   # use a saved view
hyalo views list                                    # list all views
hyalo views set <name> [find filters...]            # save a view (overwrites)
hyalo views remove <name>                           # delete a view
```

## Design

### Shared `FindFilters` struct

Extract the filter fields from `Commands::Find` into a reusable struct with dual derives — `clap::Args` (for CLI `#[command(flatten)]`) and `serde::Serialize/Deserialize` (for TOML storage):

```rust
#[derive(Debug, Clone, Default, clap::Args, serde::Serialize, serde::Deserialize)]
pub(crate) struct FindFilters {
    pub regexp: Option<String>,       // -e/--regexp
    pub properties: Vec<String>,      // -p/--property (repeatable)
    pub tag: Vec<String>,             // -t/--tag (repeatable)
    pub task: Option<String>,         // --task
    pub sections: Vec<String>,        // -s/--section (repeatable)
    pub file: Vec<String>,            // -f/--file (repeatable)
    pub glob: Vec<String>,            // -g/--glob (repeatable)
    pub fields: Vec<String>,          // --fields
    pub sort: Option<String>,         // --sort
    pub reverse: bool,                // --reverse
    pub limit: Option<usize>,         // -n/--limit
    pub broken_links: bool,           // --broken-links
    pub title: Option<String>,        // --title
}
```

Skip empty/default fields on serialization (`skip_serializing_if`) to keep TOML clean.

### Positional `pattern` stays outside `FindFilters`

The positional `PATTERN` arg on `find` would conflict with the positional `NAME` on `views set` if both were in the flattened struct. Keep `pattern` directly on `Commands::Find` (not in `FindFilters`). Views use `regexp` for body search (strictly more powerful). No breaking change.

### TOML schema (nested tables)

```toml
[views.planned-iterations]
properties = ["status=planned", "type=iteration"]
glob = ["iterations/*.md"]

[views.recent-drafts]
sort = "modified"
reverse = true
limit = 10
```

Each view value is a serialized `FindFilters`. Maps to `HashMap<String, FindFilters>` in serde.

### Merge strategy (`--view` + CLI flags)

`FindFilters::merge_from(&mut self, overlay: &FindFilters)`:
- **Vec fields** (properties, tag, sections, file, glob, fields): CLI **extends** the view
- **Option fields** (regexp, task, sort, title, limit): CLI **overrides** if Some
- **Bool fields** (reverse, broken_links): OR (CLI can turn on, not off)

### View resolution location

In `run.rs`, after config load, before dispatch — same early-return pattern as Init/Deinit. Merge by mutating `filters` in-place on the `Commands::Find` variant.

### TOML persistence

Use `toml::Table` for read-modify-write (same pattern as `init.rs`):
- Read `.hyalo.toml` as string → parse as `toml::Table`
- Insert/remove under `views.<name>`
- Serialize back with `toml::to_string`

## Tasks

- [ ] Extract `FindFilters` struct in `args.rs`, flatten into `Commands::Find` with `--view` flag
- [ ] Update `dispatch.rs` to destructure from `filters` field
- [ ] Update `run.rs` hint context to read from `filters.*`
- [ ] Verify pure refactor: `cargo check && cargo test` pass with no behavior change
- [ ] Add `views: HashMap<String, FindFilters>` to `ConfigFile` and `ResolvedDefaults` in `config.rs`
- [ ] Add `Views` command + `ViewsAction::{List, Set, Remove}` enum to `args.rs`
- [ ] Implement `commands/views.rs` — list, set, remove (TOML read/modify/write)
- [ ] Wire `Views` early dispatch in `run.rs`
- [ ] Implement `--view` merge in `run.rs` with `FindFilters::merge_from`
- [ ] Add views examples to help text
- [ ] E2E tests: views set/list/remove, find --view, find --view + overrides, unknown view error
- [ ] Code quality gates: `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

## Files to modify

| File | Change |
|------|--------|
| `crates/hyalo-cli/src/cli/args.rs` | Extract `FindFilters`; flatten into `Find`; add `--view`; add `Views` + `ViewsAction` |
| `crates/hyalo-cli/src/config.rs` | Add `views` field to `ConfigFile` + `ResolvedDefaults` |
| `crates/hyalo-cli/src/run.rs` | `Views` early dispatch; `--view` merge; hint context update |
| `crates/hyalo-cli/src/dispatch.rs` | Update `Find` destructuring for `FindFilters` |
| `crates/hyalo-cli/src/commands/mod.rs` | Add `pub mod views;` |
| `crates/hyalo-cli/src/commands/views.rs` | **New** — list, set, remove |
| `crates/hyalo-cli/src/cli/help.rs` | Add views examples |

## Verification

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# Manual:
hyalo views set planned --property status=planned --property type=iteration --glob 'iterations/*.md'
hyalo views list
hyalo find --view planned --format text
hyalo find --view planned --limit 3 --format text
hyalo views remove planned
hyalo find --view nonexistent  # should error
```
