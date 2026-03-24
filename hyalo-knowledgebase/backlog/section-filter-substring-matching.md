---
date: 2026-03-23
origin: dogfooding iter-29
priority: medium
status: planned
tags:
- backlog
- search
- cli
- ux
title: 'Section filter: support substring/prefix matching'
type: backlog
---

# Section filter: support substring/prefix matching

## Problem

`hyalo read --section` and `hyalo find --section` require an exact (case-insensitive) match on the full heading text. This is brittle when headings contain dynamic suffixes like dates, counters, or status annotations.

Example: the decision log has headings like `## DEC-031: Discoverable Drill-Down Hints Architecture (2026-03-22)`. To read that section you must pass the exact string including the date suffix. Passing just `DEC-031` or `DEC-031: Discoverable Drill-Down Hints Architecture` fails with "section not found".

This makes `--section` hard to use for:
- Decision logs where headings include dates
- Iteration files where headings include task counters like `[4/4]`
- Any heading with variable suffixes

## Proposal

Add substring or prefix matching to `SectionFilter`:
- **Option A:** Default to substring/contains matching instead of exact
- **Option B:** Add a `*` wildcard syntax, e.g. `--section "DEC-031*"`
- **Option C:** Support regex matching with a prefix, e.g. `--section "/DEC-031.*/"`

Option B is probably the best balance of power and simplicity. Option A risks false positives on short queries.

## Acceptance criteria

- `hyalo read --section "DEC-031*" --file decision-log.md` returns the full DEC-031 section
- `hyalo read --section "Tasks" --file iteration-29.md` still works (exact match unchanged)
- Error hint still shows available sections when no match is found

## My Comments
Why not introduce the same operators as in the tag ans proeprty filters: <, >, <=, >=, ~, /regexp/
See also [[property-value-search]]