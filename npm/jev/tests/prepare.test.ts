import { test, expect } from "bun:test";
import { mkdtemp, mkdir, writeFile, readFile, rm, symlink, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { policy, hash } from "../src/protocol.ts";
import { prepare, selection } from "../src/prepare.ts";
import { hyaloReader } from "../src/io.ts";
import { p, fixture } from "./fixtures.ts";
const binary = process.env.HYALO_JEV_TEST_BINARY ?? resolve(import.meta.dir, "../../../target/release/hyalo" + (process.platform === "win32" ? ".exe" : ""));
async function snapshot(root: string): Promise<Record<string, string>> {
  const result: Record<string, string> = {};
  for (const name of await readdir(root, { recursive: true, withFileTypes: true })) if (name.isFile()) {
    const path = join(name.parentPath, name.name); result[path] = hash(await readFile(path, "utf8"));
  }
  return result;
}
test("real Hyalo preparation respects bindings, exclusions, filenames, sizes and writes nothing", async () => {
  const root = await mkdtemp(join(tmpdir(), "hyalo-jev-test-")), oldCwd = process.cwd();
  try {
    await mkdir(join(root, "vault/bound"), { recursive: true });
    await writeFile(join(root, ".hyalo.toml"), 'dir = "vault"\n[schema]\nexempt = ["exempt.md"]\n[[schema.bind]]\nglob = "bound/*.md"\ntype = "docs"\n[schema.types.research]\nrequired = ["title", "type"]\n[schema.types.docs]\nrequired = ["title", "type"]\n');
    const body = "---\ntitle: Experiment\n---\n# Results\nWe measured latency in twenty trials.\n";
    for (const file of ["note.md", "space $().md", "bound/note.md", "exempt.md", "excluded.md"]) await writeFile(join(root, "vault", file), body);
    await writeFile(join(root, "vault/valid.md"), body.replace("title:", "type: docs\ntitle:"));
    await writeFile(join(root, "vault/large.md"), body + "x".repeat(20_000));
    const before = await snapshot(root); process.chdir(root);
    const m = await prepare(selection(["note.md", "space $().md", "bound/note.md", "exempt.md", "excluded.md", "valid.md", "large.md"]), policy({ version: 1, types: p.types, exclude: ["excluded.md"] }), hyaloReader(binary));
    expect(m.documents.map(d => d.file)).toEqual(["note.md", "space $().md"]);
    expect(m.deferred).toHaveLength(5); expect(await snapshot(root)).toEqual(before);
    const oldKey = process.env.TYPESAFE_API_KEY;
    try {
      process.env.TYPESAFE_API_KEY = "temporary-fixture-credential";
      await writeFile(join(root, "vault/credential.md"), body + "temporary-fixture-credential");
      const secret = await prepare(selection(["credential.md"]), p, hyaloReader(binary));
      expect(secret.documents).toHaveLength(0);
      expect(JSON.stringify(secret)).not.toContain("temporary-fixture-credential");
    } finally { if (oldKey === undefined) delete process.env.TYPESAFE_API_KEY; else process.env.TYPESAFE_API_KEY = oldKey; }
    if (process.platform !== "win32") {
      await symlink(join(root, "vault/note.md"), join(root, "vault/link.md"));
      const linked = await prepare(selection(["link.md"]), p, hyaloReader(binary)); expect(linked.deferred).toHaveLength(1);
    }
  } finally { process.chdir(oldCwd); await rm(root, { recursive: true, force: true }); }
});
test("bundle runs under Bun and Node outside checkout without dependencies", async () => {
  const root = await mkdtemp(join(tmpdir(), "hyalo-jev-bundle-"));
  try {
    const script = join(root, "jev.mjs"); await writeFile(script, await readFile(new URL("../dist/jev.mjs", import.meta.url)));
    const http = await Bun.build({ entrypoints: [resolve(import.meta.dir, "http-smoke.ts")], target: "node", format: "esm" });
    expect(http.success).toBe(true);
    const smoke = join(root, "http-smoke.mjs"); await writeFile(smoke, await http.outputs[0]!.text());
    for (const runtime of [process.execPath, "node"]) {
      const flags = runtime === process.execPath ? ["--no-install", "--no-env-file"] : [];
      const result = spawnSync(runtime, [...flags, script, "check", "-"], { cwd: root, encoding: "utf8", input: JSON.stringify(fixture()), env: { ...process.env, TYPESAFE_API_KEY: "present-but-not-consent" } });
      expect(result.status).toBe(0); expect(JSON.parse(result.stdout).network).toBe(false);
      const noConsent = spawnSync(runtime, [...flags, script, "ask", "-"], { cwd: root, encoding: "utf8", input: JSON.stringify(fixture()) });
      expect(noConsent.status).toBe(2);
      const transport = spawnSync(runtime, [...flags, smoke], { cwd: root, encoding: "utf8", timeout: 5000 });
      expect(transport.status).toBe(0);
    }
  } finally { await rm(root, { recursive: true, force: true }); }
});

test("all CLI-installed skill bundles execute without the source package", async () => {
  for (const [mode, host] of [["--claude", ".claude"], ["--codex", ".agents"], ["--pi", ".pi"]]) {
    const root = await mkdtemp(join(tmpdir(), "hyalo-jev-installed-"));
    try {
      const initialized = spawnSync(binary, ["init", mode!], { cwd: root, encoding: "utf8", env: { ...process.env, TYPESAFE_API_KEY: "present-but-not-consent" } });
      expect(initialized.status).toBe(0);
      const script = join(root, host!, "skills/hyalo-tidy/scripts/jev.mjs");
      for (const runtime of [process.execPath, "node"]) {
        const flags = runtime === process.execPath ? ["--no-install", "--no-env-file"] : [];
        const result = spawnSync(runtime, [...flags, script, "check", "-"], { cwd: root, encoding: "utf8", input: JSON.stringify(fixture()) });
        expect(result.status).toBe(0); expect(JSON.parse(result.stdout).requests).toBe(1);
      }
    } finally { await rm(root, { recursive: true, force: true }); }
  }
});
