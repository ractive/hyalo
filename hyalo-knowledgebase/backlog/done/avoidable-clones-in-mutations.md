---
title: Avoidable props.clone() in mutation commands
type: backlog
date: 2026-03-29
status: completed
origin: codebase review 2026-03-29
priority: medium
tags:
  - performance
  - refactor
---

## Problem

Five mutation commands clone the entire `IndexMap<String, Value>` when updating the snapshot index, but a simple reorder eliminates the clone:

```rust
// Current (clones):
entry.properties = props.clone();
entry.tags = extract_tags(&props);

// Fixed (moves):
let new_tags = extract_tags(&props);
entry.properties = props;  // move, no clone
entry.tags = new_tags;
```

## Locations

- `set.rs:287-288`
- `append.rs:219`
- `remove.rs:295`
- `properties.rs:187`
- `tags.rs:254`

## Acceptance criteria

- [ ] All five sites reordered to move instead of clone
- [ ] All existing tests pass
