<!-- hyalo:start -->
Use `hyalo` CLI (not Read/Grep/Glob) for all markdown knowledgebase operations.
Examples: `hyalo find --property status=planned`, `hyalo find "search text"`, `hyalo lint` (add `--strict` to fail on missing-type / undeclared-property warnings), `hyalo types list`.
Run `hyalo --help` for usage. Output format auto-detects (text on terminals, json when piped); pass `--format text`/`--format json` to override.
Use `hyalo config` to inspect the effective configuration (effective dir, config path, hints, format, site_prefix) — useful when debugging `.hyalo.toml` resolution. `--dir` selects a vault, not a config: naming the configured vault keeps `.hyalo.toml` in effect, naming another tree switches to that tree's config (announced on stderr). Its JSON is the standard `results`/`hints` envelope, so `hyalo config --jq '.results.dir'` works.
<!-- hyalo:end -->
