---
date: 2026-03-23
origin: dogfooding vscode-docs vault
priority: low
status: completed
tags:
- backlog
- cli
- filtering
- ux
title: Property value substring/regex search
type: backlog
---

# Property value substring/regex search

## Problem

Property filters only support exact match (`--property K=V`) and comparison operators. There is no way to search within property values:

- `--property 'MetaDescription=*copilot*'` does not work (wildcards not supported)
- Cannot find files where a text property contains a substring
- Cannot use regex on property values

This came up when trying to find all files whose MetaDescription mentions "copilot" without scanning body text.

## Proposal

Support a contains/regex operator:
- `--property 'MetaDescription~=copilot'` — substring match
- `--property 'MetaDescription~=/pattern/'` — regex match

## Acceptance criteria

- [ ] Can search within text property values
- [ ] Works with list properties (matches if any element matches)
- [ ] Help text documents the syntax

## My Comments
Do we really need substring match *and* regex? Substring is super easy to implement with regex. /foo/ already should be a substring filter, isnt' it?