import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, expectTypeOf, it } from "vitest";

import {
  HyaloAbortError,
  HyaloError,
  HyaloParseError,
  HyaloSpawnError,
  HyaloTimeoutError,
  HyaloTransportError,
  config,
  createPiTransport,
  execute,
  find,
  lint,
  raw,
  read,
  set,
  summary,
  task,
  type ConfigResult,
  type Envelope,
  type FileObject,
  type HyaloTransport,
  type ReadResult,
  type VaultSummary,
} from "../src/index.js";
import { isClosedStdinWriteError } from "../src/api.js";
import { configForPi } from "../src/pi-runtime.js";

const packageDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const binary = process.env.HYALO_BIN ?? path.resolve(
  packageDir,
  `../../target/release/hyalo${process.platform === "win32" ? ".exe" : ""}`,
);
let scratch = "";
let vault = "";

beforeAll(async () => {
  scratch = await mkdtemp(path.join(tmpdir(), "hyalo api contract "));
  vault = path.join(scratch, "vault with spaces");
  await mkdir(vault, { recursive: true });
  await writeFile(
    path.join(scratch, ".hyalo.toml"),
    'dir = "vault with spaces"\n[pi]\nsession_summary = true\n',
  );
  await writeFile(
    path.join(vault, "alpha.md"),
    "---\ntitle: Alpha Project\nrating: 7\nmetadata:\n  owner: Ada\ntags: [project/core]\n---\n# Alpha\nrust error handling rust\n\n## Tasks\n- [ ] ship release\n",
  );
  await writeFile(
    path.join(vault, "title only.md"),
    "---\ntitle: Rust Catalog\nnullable: null\n---\n# Catalog\nNo matching body tokens here.\n",
  );
  await writeFile(path.join(vault, "cross line.md"), "# Cross line\nalpha\nbeta\n");
  await writeFile(path.join(vault, "broken.md"), "---\ntitle: [unterminated\n---\nbody\n");
  await writeFile(path.join(vault, "plain.txt"), "not markdown\n");
  await writeFile(path.join(vault, "semi; dollar$(touch nope).md"), "# Shell safe\nexact content\n");
});

afterAll(async () => {
  if (scratch) await rm(scratch, { recursive: true, force: true });
});

const real = () => ({ binaryPath: binary, cwd: scratch }) as const;

describe("generated JSON contracts", () => {
  it("returns ranked snippets, title-only empty matches, unknown properties, and omissions", async () => {
    const response = await find({ pattern: "rust", ...real() });
    expectTypeOf(response).toEqualTypeOf<Envelope<FileObject[]>>();
    expect(response.hints).toEqual([]);
    expect(response.total).toBe(2);
    const alpha = response.results.find((item) => item.file === "alpha.md");
    const titleOnly = response.results.find((item) => item.file === "title only.md");
    expect(alpha?.score).toEqual(expect.any(Number));
    expect(alpha?.matches?.[0]).toMatchObject({ line: expect.any(Number), text: expect.stringContaining("rust") });
    expect(alpha?.properties?.rating).toBe(7);
    expect(alpha?.properties?.metadata).toEqual({ owner: "Ada" });
    expect(titleOnly?.matches).toEqual([]);
    expect("tasks" in (alpha ?? {})).toBe(false);
    expect("properties_typed" in (alpha ?? {})).toBe(false);

    const crossLine = await find({ pattern: '"alpha beta"', ...real() });
    expect(crossLine.results).toHaveLength(1);
    expect(crossLine.results[0]).toMatchObject({ file: "cross line.md", matches: [] });
  });

  it("treats positional files as file selections when no pattern is present", async () => {
    const response = await find({ file_positional: ["alpha.md"], ...real() });
    expect(response.total).toBe(1);
    expect(response.results.map((item) => item.file)).toEqual(["alpha.md"]);
  });

  it("returns body/frontmatter reads with exact nullable and optional keys", async () => {
    const body = await read({ file: ["alpha.md"], ...real() });
    expectTypeOf(body.results).toEqualTypeOf<ReadResult>();
    expect(body.results.content).toContain("rust error handling");
    expect("frontmatter" in body.results).toBe(false);
    const frontmatter = await read({ file: ["alpha.md"], frontmatter: true, ...real() });
    expect(frontmatter.results.frontmatter).toMatchObject({ rating: 7, metadata: { owner: "Ada" } });
    expect(frontmatter.results.frontmatter_raw).toContain("title: Alpha Project");
    const absent = await read({ file: ["semi; dollar$(touch nope).md"], frontmatter: true, ...real() });
    expect(absent.results.frontmatter_raw).toBeNull();
    expect(await import("node:fs").then(({ existsSync }) => existsSync(path.join(scratch, "nope")))).toBe(false);
  });

  it("hoists summary dir and preserves nested config", async () => {
    const stats = await summary({ recent: 0, depth: 0, ...real() });
    expectTypeOf(stats.results).toEqualTypeOf<Omit<VaultSummary, "dir">>();
    expect(stats.dir).toBe("vault with spaces");
    expect("dir" in stats.results).toBe(false);
    expect(stats.results.files.total).toBeGreaterThan(0);

    const current = await config(real());
    expectTypeOf(current.results).toEqualTypeOf<ConfigResult>();
    expect(current.results.pi.session_summary).toBe(true);
    expect(current.results.links).toEqual(expect.any(Object));
    expect(current.results.scan).toEqual(expect.any(Object));
    expect(current.dir).toBe("vault with spaces");
  });

  it("keeps files-from counters and treats malformed named files as exit-1 envelopes", async () => {
    const listed = await find({
      files_from: "-",
      stdin: "alpha.md\nmissing.md\nplain.txt\n../outside.md\n",
      ...real(),
    });
    expect(listed.files_missing).toBe(1);
    expect(listed.files_skipped_non_md).toBe(1);
    expect(listed.files_skipped_outside_vault).toBe(1);
    await expect(find({ file: ["broken.md"], ...real() })).rejects.toMatchObject({
      name: "HyaloError",
      exitCode: 1,
      envelope: { error: expect.stringContaining("unparseable frontmatter") },
    });
  });
});

describe("process and argv behavior", () => {
  it("preserves repeated values, empty strings, zero, paths, and forced output policy", async () => {
    let captured: readonly string[] = [];
    const transport: HyaloTransport = async (argv) => {
      captured = argv;
      return { code: 0, stdout: '{"results":[],"total":0,"hints":[]}', stderr: "" };
    };
    await find({ pattern: "", properties: ["a=1", "b=2"], glob: ["dir with spaces/*.md", "**/*.md"], limit: 0, transport });
    expect(captured).toEqual([
      "find", "--property=a=1", "--property=b=2", "--glob=dir with spaces/*.md",
      "--glob=**/*.md", "--limit=0", "--format=json", "--no-hints", "--", "",
    ]);
    expect(captured.filter((arg) => arg.startsWith("--property="))).toHaveLength(2);
  });

  it("rejects output transforms owned by typed wrappers", async () => {
    await expect(find({ format: "text" } as never)).rejects.toThrow(/'format' is reserved/);
    await expect(find({ count: true } as never)).rejects.toThrow(/'count' is reserved/);
    await expect(find({ jq: ".total" } as never)).rejects.toThrow(/'jq' is reserved/);
  });

  it("keeps exit-2 stderr, invalid and empty JSON, and spawn failure distinct", async () => {
    await expect(find({ transport: async () => ({ code: 2, stdout: "", stderr: "raw usage or internal failure" }) }))
      .rejects.toMatchObject({ name: "HyaloError", exitCode: 2, stderr: "raw usage or internal failure", envelope: undefined });
    await expect(find({ transport: async () => ({ code: 0, stdout: "not json", stderr: "diagnostic" }) }))
      .rejects.toBeInstanceOf(HyaloParseError);
    await expect(find({ transport: async () => ({ code: 0, stdout: "", stderr: "" }) }))
      .rejects.toThrow("empty JSON");
    await expect(find({ binaryPath: path.join(scratch, "missing executable") }))
      .rejects.toBeInstanceOf(HyaloSpawnError);
  });

  it("extracts a trailing error envelope while preserving config warnings", async () => {
    const malformed = path.join(scratch, "malformed config");
    await mkdir(path.join(malformed, "vault"), { recursive: true });
    await writeFile(path.join(malformed, ".hyalo.toml"), "not = [valid\n");
    const failure = await read({
      dir: "vault",
      file: ["missing.md"],
      binaryPath: binary,
      cwd: malformed,
    }).catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(HyaloError);
    expect(failure).toMatchObject({
      exitCode: 1,
      envelope: { error: "file not found", path: "missing.md" },
    });
    expect((failure as HyaloError).stderr).toContain("warning: malformed .hyalo.toml");
    expect((failure as HyaloError).stderr).toContain('"error": "file not found"');
  });

  it("times out, aborts, and preserves exit results after closed-stdin errors", async () => {
    expect(isClosedStdinWriteError({ code: "EPIPE" })).toBe(true);
    expect(isClosedStdinWriteError({ code: "EOF" })).toBe(true);
    expect(isClosedStdinWriteError({ code: "ECONNRESET" })).toBe(false);
    expect(isClosedStdinWriteError({ code: undefined })).toBe(false);
    await expect(raw(["-e", "setTimeout(() => {}, 10000)"], { binaryPath: process.execPath, timeoutMs: 20 }))
      .rejects.toBeInstanceOf(HyaloTimeoutError);
    const controller = new AbortController();
    const pending = raw(["-e", "setTimeout(() => {}, 10000)"], {
      binaryPath: process.execPath,
      timeoutMs: 5_000,
      signal: controller.signal,
    });
    controller.abort();
    await expect(pending).rejects.toBeInstanceOf(HyaloAbortError);
    const early = await raw(["-e", "process.stderr.write('early exit'); process.exit(7)"], {
      binaryPath: process.execPath,
      stdin: "x".repeat(16 * 1024 * 1024),
    });
    expect(early).toMatchObject({ code: 7, stderr: "early exit" });
  });
});

describe("Pi adapters", () => {
  it("discovers legacy vaults, defaults summaries off, and retains lint findings", async () => {
    const calls: string[][] = [];
    const transport: HyaloTransport = async (argv) => {
      calls.push([...argv]);
      if (argv[0] === "config") {
        return {
          code: 0,
          stdout: JSON.stringify({
            dir: "/legacy/vault",
            results: {
              config_path: "/legacy/.hyalo.toml",
              raw_contents: null,
              cwd: "/legacy",
              dir_overridden: false,
              format: null,
              hints: true,
              site_prefix: null,
              exempt: [],
            },
            hints: [],
          }),
          stderr: "",
        };
      }
      return { code: 1, stdout: "HYALO001 legacy finding", stderr: "" };
    };
    await expect(configForPi({ transport })).resolves.toEqual({
      vaultDir: "/legacy/vault",
      sessionSummary: false,
    });
    await expect(lint("note.md", { transport })).resolves.toMatchObject({
      code: 1,
      stdout: "HYALO001 legacy finding",
    });
    expect(calls).toContainEqual(["lint", "--format=text", "--no-hints", "--", "note.md"]);

    await expect(configForPi({
      transport: async () => ({
        code: 0,
        stdout: '{"dir":"/flat/vault","hints":true}',
        stderr: "",
      }),
    })).resolves.toEqual({ vaultDir: "/flat/vault", sessionSummary: false });

    await expect(configForPi({
      transport: async () => ({
        code: 0,
        stdout: '{"dir":"/modern/vault","results":{"dir":"/modern/vault","pi":{"session_summary":true}},"hints":[]}',
        stderr: "",
      }),
    })).resolves.toEqual({ vaultDir: "/modern/vault", sessionSummary: true });
  });

  it("maps Pi killed results and rejects stdin before exec", async () => {
    let calls = 0;
    const pi = {
      exec: async () => {
        calls += 1;
        return { code: 0, stdout: "", stderr: "", killed: true };
      },
    };
    const transport = createPiTransport(pi);
    await expect(raw(["find"], { transport, timeoutMs: 17 }))
      .rejects.toBeInstanceOf(HyaloTimeoutError);

    const controller = new AbortController();
    controller.abort();
    await expect(raw(["find"], { transport, signal: controller.signal }))
      .rejects.toBeInstanceOf(HyaloAbortError);

    const beforeStdin = calls;
    await expect(find({ files_from: "-", stdin: "alpha.md\n", transport }))
      .rejects.toBeInstanceOf(HyaloTransportError);
    expect(calls).toBe(beforeStdin);
  });

  it("uses injected transports for config, set, task and lint exit-1 findings", async () => {
    const calls: string[][] = [];
    const transport: HyaloTransport = async (argv) => {
      calls.push([...argv]);
      if (argv[0] === "config") return { code: 0, stdout: '{"results":{"pi":{"session_summary":true}},"hints":[]}', stderr: "" };
      if (argv[0] === "lint") return { code: 1, stdout: "HYALO001 finding", stderr: "" };
      return { code: 0, stdout: "changed", stderr: "" };
    };
    await config({ transport });
    await set({ file: "space name.md", property: "rank=0", tag: "", transport });
    await task({ file: "space name.md", mode: "line", lines: [2.9, 4], transport });
    expect((await lint("space name.md", { transport })).code).toBe(1);
    expect(calls).toContainEqual(["set", "--format=text", "--property=rank=0", "--tag=", "--", "space name.md"]);
    expect(calls).toContainEqual(["task", "toggle", "--line=2,4", "--", "space name.md"]);
    const beforeInvalid = calls.length;
    await expect(task({ file: "space name.md", mode: "typo" as never, transport }))
      .rejects.toThrow("unknown task mode 'typo'");
    expect(calls).toHaveLength(beforeInvalid);
  });

  it("mutates an isolated real fixture through set/task", async () => {
    const mutable = path.join(vault, "mutable.md");
    await writeFile(mutable, "---\nstatus: planned\n---\n# Work\n- [ ] first\n");
    expect((await set({ file: "mutable.md", property: "status=active", ...real() })).code).toBe(0);
    expect((await task({ file: "mutable.md", mode: "all", ...real() })).code).toBe(0);
    const result = await read({ file: ["mutable.md"], frontmatter: true, ...real() });
    expect(result.results.frontmatter).toMatchObject({ status: "active" });
    expect((await read({ file: ["mutable.md"], ...real() })).results.content).toContain("- [x] first");
  });

  it("treats option-looking files and leading-hyphen values literally", async () => {
    const crafted = path.join(vault, "--glob=control.md");
    const control = path.join(vault, "control.md");
    await writeFile(crafted, "---\nstatus: planned\n---\n# Crafted\n");
    await writeFile(control, "---\nstatus: planned\n---\n# Control\n");
    await set({ file: "--glob=control.md", property: "status=-active", ...real() });
    const changed = await read({ file: ["--glob=control.md"], frontmatter: true, ...real() });
    const untouched = await read({ file: ["control.md"], frontmatter: true, ...real() });
    expect(changed.results.frontmatter).toMatchObject({ status: "-active" });
    expect(untouched.results.frontmatter).toMatchObject({ status: "planned" });
  });
});
