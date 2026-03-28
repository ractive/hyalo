---
date: 2026-03-21
status: completed
tags:
- cli
- output
- ux
- structured-data
- filtering
title: CLI Structured Output & Filtering Patterns
type: research
---

# CLI Structured Output & Filtering Patterns

Research into how popular CLIs expose structured output and filtering to users.

## Per-CLI Breakdown

### kubectl

- **Format flag:** `-o` / `--output`
- **Formats:** `json`, `yaml`, `wide`, `name`, `jsonpath=EXPR`, `custom-columns=SPEC`, `go-template=TMPL`, `go-template-file=FILE`
- **Query language:** JSONPath (Kubernetes dialect — adds `range`/`end` iteration, no regex support)
- **Format + filter:** Combined in single flag — `--output=jsonpath='{.items[*].metadata.name}'`
- **Notable:** JSONPath expression is embedded in the `-o` flag value itself, not a separate flag. Also supports `--sort-by` for ordering.
- **Examples:**
  - `kubectl get pods -o json`
  - `kubectl get pods -o jsonpath='{.items[*].metadata.name}'`
  - `kubectl get pods -o custom-columns=NAME:.metadata.name,STATUS:.status.phase`
  - `kubectl get pods -o go-template='{{range .items}}{{.metadata.name}}{{"\n"}}{{end}}'`

### gh (GitHub CLI)

- **Format flag:** `--json <field1,field2,...>` (selects fields, outputs JSON)
- **Filter flag:** `--jq <expression>` (separate flag, jq syntax)
- **Template flag:** `--template <string>` (separate flag, Go templates)
- **Query language:** jq (for filtering), Go templates (for formatting)
- **Format + filter:** Separate flags that compose: `--json` selects fields, then `--jq` or `--template` transforms them
- **Notable:** `--json` is required before `--jq` or `--template` can be used. The field list in `--json` acts as a projection. This is the most composable design.
- **Examples:**
  - `gh pr list --json number,title,author`
  - `gh pr list --json number,title --jq '.[] | select(.number > 100)'`
  - `gh pr list --json number,title --template '{{range .}}{{.number}}: {{.title}}{{"\n"}}{{end}}'`

### aws cli

- **Format flag:** `--output json|yaml|yaml-stream|text|table|off`
- **Filter flag:** `--query <JMESPath expression>` (separate flag)
- **Query language:** JMESPath
- **Format + filter:** Separate flags that compose independently
- **Notable:** `--query` interacts differently with `--output text` (runs per-page) vs JSON/YAML (runs once on full result). `text` output is tab-separated, good for piping to Unix tools. `off` suppresses output entirely (useful for CI/CD).
- **Examples:**
  - `aws ec2 describe-instances --output table`
  - `aws iam list-users --output text --query 'Users[*].[UserName,Arn]'`
  - `aws ec2 describe-instances --query 'Reservations[*].Instances[*].[InstanceId,State.Name]' --output json`

### docker

- **Format flag:** `--format <TEMPLATE>` / `-f <TEMPLATE>`
- **Formats:** `json` (special keyword) or Go template string
- **Query language:** Go templates (text/template package)
- **Format + filter:** Combined in single flag — the Go template both selects and formats
- **Notable:** Separate `--type` flag filters *what* to inspect, not *how* to display. Has a special `json` function inside templates for sub-objects: `{{json .Config}}`.
- **Examples:**
  - `docker inspect --format='{{.Config.Image}}' $ID`
  - `docker inspect --format='{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' $ID`
  - `docker ps --format 'table {{.Names}}\t{{.Status}}'`
  - `docker inspect -f 'json' $ID`

### jc

- **Not a CLI with built-in formatting** — it is a *converter* that sits in a pipeline
- **Usage:** `COMMAND | jc PARSER | jq FILTER`
- **Alternative:** `jc COMMAND` (magic syntax, wraps the command)
- **Formats:** JSON output (with `-r` for raw/pre-processed)
- **100+ parsers** for common commands (ps, ls, dig, netstat, df, etc.) and file formats (CSV, XML, YAML, /etc/passwd)
- **Notable:** Demonstrates the "pipeline" approach — structured output is a *post-processing step* rather than built into each CLI. Works well with jq downstream.

### gcloud

- **Format flag:** `--format=FORMAT(PROJECTION)`
- **Formats:** `json`, `yaml`, `table`, `csv`, `value`, `flattened`, `get`, `list`, `config`, `diff`, `object`, `default`, `none`, `disable`
- **Filter flag:** `--filter=EXPRESSION` (separate flag, gcloud's own filter language)
- **Query language:** gcloud's own projection/filter DSL (not JMESPath, not jq)
- **Format + filter:** Separate flags. `--format` controls display *and* can include inline projections: `--format='table(name, zone, status)'`. `--filter` controls which resources are returned.
- **Notable:** The most complex system. Format and projection are combined in the `--format` flag via parenthesized field lists. `--filter` uses a proprietary expression language with operators like `=`, `!=`, `~` (regex), `AND`/`OR`. Also supports `--flatten` for denormalizing nested arrays.
- **Examples:**
  - `gcloud compute instances list --format=json`
  - `gcloud compute instances list --format='table(name, zone, status)'`
  - `gcloud compute instances list --format='csv(name, zone)' --filter='status=RUNNING'`
  - `gcloud compute instances list --format='value(name)'`

### az (Azure CLI)

- **Format flag:** `--output` / `-o`
- **Formats:** `json` (default), `jsonc` (colorized), `yaml`, `yamlc` (colorized), `table`, `tsv`, `none`
- **Filter flag:** `--query <JMESPath expression>` (separate flag)
- **Query language:** JMESPath (same as AWS CLI)
- **Format + filter:** Separate flags that compose independently
- **Notable:** Very similar to AWS CLI design. `tsv` output has no guaranteed column ordering unless you use `--query` to project explicitly. `none` suppresses output. Default can be set globally via `az config set core.output=table`. `jsonc`/`yamlc` are nice touches for terminal readability.
- **Examples:**
  - `az vm list --output table`
  - `az vm list --query "[].{resource:resourceGroup, name:name}" --output table`
  - `az vm list --output tsv --query '[].[id, location, resourceGroup, name]'`

### steampipe / powerpipe

- **Format flag:** `--output`
- **Formats:** `pretty` (default), `plain`, `brief`, `json`, `csv`, `html`, `md`, `nunit3`, `none`, `pps`/`snapshot`, `asff`
- **No built-in query/filter flag** — filtering happens in the SQL query itself
- **Notable:** Steampipe's paradigm is "SQL is the query language" — users write SQL to filter and project data, so the CLI only needs an output format flag. Also has `--export` for saving to files in multiple formats simultaneously.
- **Examples:**
  - `steampipe query "select * from aws_s3_bucket" --output json`
  - `powerpipe benchmark run cis_v120 --output csv`

---

## Pattern Summary

### Flag Naming Conventions

| Pattern | Used by | Flag |
|---------|---------|------|
| `--output` / `-o` | kubectl, aws, az, steampipe | Most common for format selection |
| `--format` | docker, gcloud | Used when format includes templating |
| `--json` | gh | Dual-purpose: enables JSON mode + selects fields |
| `--query` | aws, az | For JMESPath filtering |
| `--jq` | gh | For jq filtering |
| `--filter` | gcloud | For resource-level filtering |
| `--template` | gh | For Go template formatting |

### Format vs Filter: Separate or Combined?

| Approach | CLIs | Pros | Cons |
|----------|------|------|------|
| **Separate flags** (`--output` + `--query`) | aws, az | Clean separation of concerns; each flag does one thing | Two flags to learn |
| **Combined in format flag** (`-o jsonpath=EXPR`) | kubectl | Fewer flags, compact | Expression embedded in flag value, less composable |
| **Three-stage pipeline** (`--json` + `--jq`/`--template`) | gh | Most composable; field selection, filtering, and formatting are independent | Three flags to learn |
| **Format with inline projection** (`--format='table(f1,f2)'`) | gcloud | Projection is part of display intent | Complex syntax, proprietary DSL |
| **External pipeline** (`| jc | jq`) | jc | No changes to the CLI itself | Requires external tools |

### Query Language Adoption

| Language | Used by | Complexity | Ecosystem |
|----------|---------|------------|-----------|
| **JMESPath** | aws, az | Medium | Python-native, libraries in many languages |
| **jq** | gh (via --jq) | High | Ubiquitous Unix tool, very powerful |
| **JSONPath** | kubectl | Medium | Many variants, Kubernetes has its own dialect |
| **Go templates** | docker, gh, kubectl | Medium | Native to Go ecosystem |
| **Proprietary DSL** | gcloud | High | Only works with gcloud |
| **SQL** | steampipe | High (but familiar) | Universal |

### Best Practices Observed

1. **Default to JSON** — aws, az, gh all default to or prominently support JSON. It is the lingua franca of structured CLI output.

2. **Separate format from filter** — The aws/az pattern (`--output` + `--query`) is the most widely replicated. It keeps concerns clean: *what shape* vs *what data*.

3. **Use an established query language** — JMESPath (aws, az) and jq (gh) are the two leaders. Proprietary DSLs (gcloud) create learning burden. JSONPath has too many incompatible dialects.

4. **Offer a human-readable default** — `table` or `text` for interactive use, `json` for scripting. Let users set a global default (az does this well).

5. **Provide `tsv`/`text` for shell scripting** — Tab-separated output pipes cleanly into `cut`, `awk`, `xargs`. aws and az both support this.

6. **Provide `none`/`off` for CI/CD** — Suppress output when only the exit code matters (aws `off`, az `none`).

7. **gh's three-stage design is the most elegant** — `--json fields` (projection) + `--jq expr` (filter/transform) + `--template tmpl` (display) cleanly separates concerns while remaining composable.

8. **Keep format flag values simple** — Prefer `--output json` over `--output=jsonpath='{complex.expression}'`. When expressions are needed, put them in a separate flag.

### Common Output Formats Across CLIs

| Format | Support |
|--------|---------|
| JSON | All 8 CLIs |
| YAML | kubectl, aws, az, gcloud |
| Table | aws, az, gcloud, docker, steampipe |
| CSV | gcloud, steampipe |
| TSV | az |
| Plain text | kubectl (wide), aws (text) |
| None/Off | aws, az, steampipe |
