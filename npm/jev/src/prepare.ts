import { lstat, realpath } from "node:fs/promises";
import { resolve, relative as pathRelative, sep } from "node:path";
import { type HyaloRead } from "./io.ts";
import { type Manifest, type Document, type Deferral, type Policy, LIMITS, Invalid, insist, object, keys, relative, string, hash, evidenceHash, payload, manifest } from "./protocol.ts";

export interface Selection { file: string; section: string | null }
export function selection(value: unknown): Selection[] {
  insist(Array.isArray(value) && value.length > 0 && value.length <= LIMITS.documents, "select 1–25 explicit documents");
  const files = value.map(v => {
    if (typeof v === "string") return { file: relative(v), section: null };
    const d = object(v); keys(d, ["file", "section"]);
    return { file: relative(d.file), section: d.section === undefined ? null : string(d.section, 200) };
  });
  insist(new Set(files.map(d => d.file)).size === files.length, "duplicate selection");
  insist(files.every(d => d.file.endsWith(".md")), "select Markdown files");
  return files;
}

async function inside(root: string, path: string, directory: boolean) {
  const target = resolve(root, path), resolved = await realpath(target);
  const rel = pathRelative(root, resolved);
  insist(rel !== ".." && !rel.startsWith(".." + sep), "path escapes vault");
  let component = root;
  for (const part of path.split("/")) {
    component = resolve(component, part);
    insist(!(await lstat(component)).isSymbolicLink(), "symlink requires manual review");
  }
  const stat = await lstat(target);
  insist(directory ? stat.isDirectory() : stat.isFile(), "unexpected path kind");
}

export async function prepare(files: Selection[], p: Policy, read: HyaloRead): Promise<Manifest> {
  const config = object((await read(["config", "--raw"])).results);
  insist(config.malformed === false && config.dir_out_of_bounds === false && !config.schema_error, "configuration requires repair");
  const root = await realpath(resolve(string(config.cwd), string(config.dir)));
  const schemas = Object.fromEntries(await Promise.all(p.types.map(async c => [c.value, object((await read(["types", "show", c.value])).results)])));
  for (const c of p.folders) await inside(root, c.value, true);
  const contextHash = hash({ config, schemas });
  const documents: Document[] = [], deferred: Deferral[] = [];
  for (const selected of files) {
    try {
      if (p.exclude.some(x => selected.file === x || selected.file.startsWith(x + "/"))) throw new Invalid("excluded by policy");
      await inside(root, selected.file, false);
      const found = await read(["find", "--file", selected.file, "--fields", "size", "--limit", "2"]);
      insist(found.total === 1 && Array.isArray(found.results) && found.results.length === 1, "document excluded or unavailable");
      const entry = object(found.results[0]);
      insist(entry.file === selected.file, "file resolution differs from selection");
      insist(typeof entry.size === "number" && entry.size <= LIMITS.input && (selected.section !== null || entry.size <= 18_000), "document too large; select a relevant section");
      const readArgs = selected.section === null ? ["--lines", "1:"] : ["--section", selected.section];
      const evidence = object((await read(["read", "--file", selected.file, "--frontmatter", ...readArgs])).results);
      insist(evidence.file === selected.file, "file resolution differs from selection");
      const current = object(evidence.frontmatter ?? {}), content = string(evidence.content, 18_000);
      let missingType = false;
      if (!("type" in current) && p.types.length) {
        // Use Hyalo's effective schema resolution, including path bindings,
        // exemptions and lint ignores. Absence of this positive signal defers.
        const lint = object((await read(["lint", "--file", selected.file, "--rule", "SCHEMA", "--strict", "--detailed"], true)).results);
        const entries = Array.isArray(lint.files) ? lint.files : [];
        // Hyalo 0.24 omits the internal violation kind from its public JSON.
        // Unknown wording defers safely instead of assuming a missing type.
        missingType = entries.some(v => {
          const file = object(v);
          return file.file === selected.file && Array.isArray(file.rule_groups) && file.rule_groups.some(g => {
            const group = object(g);
            return group.rule === "SCHEMA" && Array.isArray(group.violations) && group.violations.some(v => object(v).message === "no 'type' property — validating against default schema only");
          });
        });
        if (!missingType) deferred.push({ file: selected.file, reason: "type is bound, exempt, ignored, or not confirmed missing" });
      }
      const doc = { file: selected.file, section: selected.section, content, current, missingType };
      const full = { ...doc, fingerprint: evidenceHash(doc, p, contextHash) };
      const request = payload(full, p);
      if (Object.keys(request.request.questions).length || (typeof current.type === "string" && p.typeFolders[current.type])) {
        // A document can have deferred fields and usable independent questions.
        if (deferred.at(-1)?.file === selected.file) deferred.pop();
        documents.push(full);
      } else if (deferred.at(-1)?.file !== selected.file) deferred.push({ file: selected.file, reason: "no eligible missing values or filing questions" });
    } catch (error) {
      if (deferred.at(-1)?.file === selected.file) deferred.pop();
      deferred.push({ file: selected.file, reason: error instanceof Invalid ? error.message : "document unavailable" });
    }
  }
  // A changed config/schema invalidates the complete preparation pass.
  insist(hash(object((await read(["config", "--raw"])).results)) === hash(config), "configuration changed during preparation");
  return manifest({ version: 1, policy: p, contextHash, documents, deferred, selected: files.length });
}
