import { test, expect } from "bun:test";
import { mkdtemp, mkdir, writeFile, readFile, rm, stat, link } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { policy, LIMITS, payload } from "../src/protocol.ts";
import { prepare, selection } from "../src/prepare.ts";
import { hyaloReader } from "../src/io.ts";
import { ask } from "../src/provider.ts";

const binary = process.env.HYALO_JEV_TEST_BINARY ?? resolve(import.meta.dir, "../../../target/release/hyalo" + (process.platform === "win32" ? ".exe" : ""));
const body = "---\ntitle: Experiment\n---\n# Results\nWe measured latency.\n";
const tags = policy({ version: 1, tags: [{ value: "performance", description: "Measurements of performance." }] });

async function vault() {
  const root = await mkdtemp(join(tmpdir(), "hyalo-jev-contract-"));
  await mkdir(join(root, "vault"));
  await writeFile(join(root, ".hyalo.toml"), 'dir = "vault"\n[schema.types.docs]\nrequired = ["title", "type"]\n');
  return root;
}

test("filesystem identities enforce exclusions without folding distinct case-sensitive paths", async () => {
  const root = await vault(), oldCwd = process.cwd();
  try {
    await mkdir(join(root, "vault/private"));
    await mkdir(join(root, "vault/PRIVATE"), { recursive: true });
    for (const name of ["private/note.md", "PRIVATE/note.md", "private/NOTE.md"]) await writeFile(join(root, "vault", name), body);
    await link(join(root, "vault/private/note.md"), join(root, "vault/alias.md"));
    process.chdir(root);
    for (const [file, excluded, identityPath] of [
      ["PRIVATE/note.md", "private", "PRIVATE"],
      ["private/NOTE.md", "private/note.md", "private/NOTE.md"],
      ["alias.md", "private/note.md", "alias.md"],
    ] as const) {
      const candidate = await stat(join(root, "vault", identityPath), { bigint: true });
      const exclusion = await stat(join(root, "vault", excluded), { bigint: true });
      const same = candidate.dev === exclusion.dev && candidate.ino === exclusion.ino;
      const m = await prepare(selection([file]), policy({ ...tags, exclude: [excluded] }), hyaloReader(binary));
      expect(m.documents).toHaveLength(same ? 0 : 1);
      if (same) {
        expect(m.deferred[0]?.reason).toBe("excluded by policy");
        let requests = 0;
        await ask(m, { allowNetwork: true, apiKey: "test-key", fetch: async () => { requests++; throw new Error("unexpected request"); } });
        expect(requests).toBe(0);
      }
    }
  } finally { process.chdir(oldCwd); await rm(root, { recursive: true, force: true }); }
});

test("valid Hyalo type representations map locally and preserve original metadata", async () => {
  const root = await vault(), oldCwd = process.cwd();
  try {
    await mkdir(join(root, "vault/docs"));
    const types = ["docs", " docs ", "[[docs]]", "[[types/docs#Overview|Manual]]", ["docs"], ["[[docs|Manual]]"]];
    const files = types.map((_, i) => `note-${i}.md`);
    for (const [i, type] of types.entries()) await writeFile(join(root, "vault", files[i]!), body.replace("title:", `type: ${JSON.stringify(type)}\ntitle:`));
    process.chdir(root);
    const p = policy({ version: 1, types: [{ value: "docs", description: "Documentation." }], folders: [{ value: "docs", description: "Documentation." }], typeFolders: { docs: "docs" } });
    const m = await prepare(selection(files), p, hyaloReader(binary));
    expect(m.documents.map(d => d.current.type)).toEqual(types);
    let requests = 0;
    const result = await ask(m, { allowNetwork: true, apiKey: "test-key", fetch: async () => { requests++; throw new Error("unexpected request"); } });
    expect(requests).toBe(0);
    expect(result.results.map(r => r.decisions)).toEqual(types.map(() => [{ field: "folder", status: "suggestion", value: "docs", reason: "local type-to-folder convention" }]));
  } finally { process.chdir(oldCwd); await rm(root, { recursive: true, force: true }); }
});

test("existing scalar and case-variant tags produce no paid questions", async () => {
  const root = await vault(), oldCwd = process.cwd();
  try {
    const values = ["performance", "Performance", ["PERFORMANCE"]];
    const files = values.map((_, i) => `note-${i}.md`);
    for (const [i, value] of values.entries()) await writeFile(join(root, "vault", files[i]!), body.replace("title:", `tags: ${JSON.stringify(value)}\ntitle:`));
    const before = await Promise.all(files.map(file => readFile(join(root, "vault", file), "utf8")));
    process.chdir(root);
    const m = await prepare(selection(files), tags, hyaloReader(binary));
    expect(m.documents).toHaveLength(0);
    expect(m.deferred).toHaveLength(3);
    expect(await Promise.all(files.map(file => readFile(join(root, "vault", file), "utf8")))).toEqual(before);
    // Hyalo append folds ASCII only; different non-ASCII strings stay distinct.
    const p = policy({ version: 1, tags: [{ value: "ä", description: "A local category." }] });
    const d = { file: "note.md", section: null, content: "Evidence", current: { tags: ["Ä"] }, missingType: false, fingerprint: "" };
    expect(Object.keys(payload(d, p).request.questions)).toEqual(["tag0"]);
  } finally { process.chdir(oldCwd); await rm(root, { recursive: true, force: true }); }
});

test("large-frontmatter batches defer overflow and their emitted manifest passes both runtimes", async () => {
  const root = await vault(), oldCwd = process.cwd();
  try {
    const files = Array.from({ length: 25 }, (_, i) => ({ file: `note-${i}.md`, section: "Results" }));
    for (const { file } of files) await writeFile(join(root, "vault", file), body.replace("title:", `data: ${"x".repeat(50_000)}\ntitle:`));
    process.chdir(root);
    const m = await prepare(selection(files), tags, hyaloReader(binary));
    expect(m.documents.length).toBeGreaterThan(0);
    expect(m.deferred.length).toBeGreaterThan(0);
    expect(m.documents.length + m.deferred.length).toBe(25);
    expect(m.deferred.every(d => d.reason.includes("manifest budget"))).toBe(true);
    const input = JSON.stringify(m) + "\n";
    expect(Buffer.byteLength(input)).toBeLessThanOrEqual(LIMITS.input);
    for (const runtime of [process.execPath, "node"]) {
      const flags = runtime === process.execPath ? ["--no-install", "--no-env-file"] : [];
      const checked = spawnSync(runtime, [...flags, resolve(import.meta.dir, "../dist/jev.mjs"), "check", "-"], { input, encoding: "utf8", cwd: root });
      expect(checked.status).toBe(0);
      expect(JSON.parse(checked.stdout).eligible).toBe(m.documents.length);
    }
  } finally { process.chdir(oldCwd); await rm(root, { recursive: true, force: true }); }
}, 15_000);
