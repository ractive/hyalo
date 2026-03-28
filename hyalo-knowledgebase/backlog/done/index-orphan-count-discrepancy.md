---
title: Snapshot index computes different orphan count than disk scan
type: backlog
date: 2026-03-28
status: completed
priority: critical
origin: dogfooding v0.4.2 on vscode-docs/docs
---

`hyalo summary` reports **48 orphans** from disk scan but only **25 orphans** from the snapshot index on vscode-docs/docs (339 files). 23 files silently disappear from the orphan list when using `--index`.

Missing orphans include: `cpp/enable-logging-cpp.md`, `cpp/natvis.md`, `csharp/signing-in.md`, `csharp/testing.md`, `copilot/guides/mcp-developer-guide.md`, `datascience/data-wrangler.md`, `enterprise/policies.md`, `getstarted/copilot-quickstart.md`, `intelligentapps/agent-inspector.md`, `intelligentapps/tracing.md`, `intelligentapps/reference/FileStructure.md`, `intelligentapps/reference/SetupWithoutAITK.md`, `intelligentapps/reference/TemplateProject.md`, `java/java-linting.md`, `java/java-refactoring.md`, `languages/powershell.md`, `languages/tsql.md`, `nodejs/nodejs-deployment.md`, `python/python-on-azure.md`, `setup/network.md`, `sourcecontrol/repos-remotes.md`, `supporting/oss-extensions.md`.

**Root cause hypothesis:** The `SnapshotIndex` link graph likely resolves links differently than `ScannedIndex` — possibly including more link types, handling relative paths differently, or normalizing paths in a way that creates false positives for reachability.

**Reproduction:**
```bash
hyalo summary --dir ../vscode-docs/docs
hyalo create-index --dir ../vscode-docs/docs
hyalo summary --dir ../vscode-docs/docs --index ../vscode-docs/docs/.hyalo-index
# Compare orphan counts and lists
```

**Fix:** Diff the link graphs from both code paths to find the divergence point. Ensure `SnapshotIndex::link_graph()` produces identical results to `ScannedIndex::link_graph()`.
