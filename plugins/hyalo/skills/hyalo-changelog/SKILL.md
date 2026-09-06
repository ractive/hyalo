---
name: hyalo-changelog
description: Maintain Keep a Changelog files with Hyalo's changelog profile and add/release commands.
---

<!-- hyalo:managed -->

# Changelog maintenance

Inspect `hyalo config` and `hyalo changelog --help` before editing.
The configured changelog may be outside the notes directory, relative to the project
configuration. Follow that path rather than assuming a vault-local CHANGELOG.md.

Use `hyalo changelog add --help` or `hyalo changelog release --help` for the exact
arguments. Preserve the existing Keep a Changelog sections and release history.
Moving entries into a release requires the requested version and release scope;
do not invent a release or publish artifacts while writing release notes.
Use `hyalo lint --profile changelog --format text` to inspect conformance.
