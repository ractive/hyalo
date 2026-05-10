<!-- hyalo:start -->
Use `hyalo` CLI (not Read/Grep/Glob) for all markdown knowledgebase operations.
Examples: `hyalo find --property status=planned`, `hyalo find "search text"`, `hyalo lint` (add `--strict` to fail on missing-type / undeclared-property warnings), `hyalo types list`.
Run `hyalo --help` for usage. Output format auto-detects (text on terminals, json when piped); pass `--format text`/`--format json` to override.
Use `hyalo config` to inspect the effective configuration (resolved dir, config path, hints, format, site_prefix) — useful when debugging `.hyalo.toml` resolution.
<!-- hyalo:end -->
