---
name: changelog
user_invocable: false
description: >
  Author and maintain a Keep a Changelog 1.1.0 `CHANGELOG.md` with hyalo. Use this skill
  whenever you are editing, validating, or releasing a changelog — the human-readable
  `CHANGELOG.md` at a project root that records notable changes grouped under version
  sections. Trigger it when: adding a changelog entry, cutting a release (rotating
  `## [Unreleased]` into a dated version), validating the changelog grammar, or fixing
  version/date ordering and footer link references. Even if the user does not say
  "changelog" by name, use this skill when the task involves a `CHANGELOG.md` with
  `## [Unreleased]` / `## [X.Y.Z] - DATE` sections.
---

# Keep a Changelog — Authoring with hyalo

[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) is a convention for a
human-readable `CHANGELOG.md`. hyalo owns the deterministic mechanics (grammar validation,
release rotation, footer link references); the LLM writes the change descriptions.

## Model

- **Title**: a single `# Changelog` H1 at the top.
- **Version sections** (H2): `## [Unreleased]` pinned first, then
  `## [X.Y.Z] - YYYY-MM-DD` sections, newest first (versions strictly descending, dates
  non-increasing). A yanked release is marked `## [X.Y.Z] - YYYY-MM-DD [YANKED]`.
- **Category subsections** (H3): limited to `Added`, `Changed`, `Deprecated`, `Removed`,
  `Fixed`, `Security`.
- **Footer**: a block of `[x.y.z]: <url>` link-reference definitions — one per version
  heading (including `[Unreleased]`), typically comparison/tag URLs.

`CHANGELOG.md` is **frontmatter-free**. The `changelog` profile binds a `changelog` type to
the literal `CHANGELOG.md` and exempts it from the frontmatter rules, so the grammar is
enforced by the `CHANGELOG-*` lint rules, not the schema pass.

## Validate (`hyalo lint --profile changelog`)

```
hyalo lint --profile changelog        # validate CHANGELOG.md against the 1.1.0 grammar
```

The rules (mostly **error**-severity, since a malformed changelog is a real defect):

- `CHANGELOG-TITLE` — the file must start with `# Changelog`.
- `CHANGELOG-VERSION-HEADING` — H2 headings must be `[Unreleased]` or `[X.Y.Z] - DATE`.
- `CHANGELOG-CATEGORY` — H3 headings must be one of the six categories.
- `CHANGELOG-VERSION-ORDER` — versions strictly descending (newest first).
- `CHANGELOG-DATE-ORDER` — release dates non-increasing.
- `CHANGELOG-UNRELEASED-POSITION` — `[Unreleased]` must be the first version section.
- `CHANGELOG-EMPTY-SECTION` (warn) — a released/category section with no content.
- `CHANGELOG-LINK-REF` (warn) — a version heading without a footer link ref, or vice versa.

Every rule respects `[lint.rules.<id>]` overrides and appears in `hyalo lint-rules list`.

## Release generator (`hyalo changelog release`)

```
hyalo changelog add --category Added --message "New export format"   # append under Unreleased
hyalo changelog release 1.2.0                                        # dry-run: preview rotation
hyalo changelog release 1.2.0 --apply                               # rotate [Unreleased] → [1.2.0]
hyalo changelog release 1.2.0 --date 2026-07-17 --apply            # override the date
```

- **`changelog add`** appends `- <message>` under the `### <category>` subsection of
  `## [Unreleased]`, creating the subsection if missing.
- **`changelog release <X.Y.Z>`** rotates the accumulated `## [Unreleased]` content into a
  dated `## [X.Y.Z] - <date>` section (date defaults to today), re-creates an empty
  `[Unreleased]` above it, and appends a placeholder `[X.Y.Z]: TBD` footer link reference
  (fill in the real compare URL). It **refuses** to release a version that already exists
  (idempotency guard). Both default to `--dry-run` and exit non-zero on drift, so they
  double as CI checks; pass `--apply` to write.

After a release, replace the `TBD` link target with the real compare/tag URL and run
`hyalo lint --profile changelog` to confirm the result is clean.
