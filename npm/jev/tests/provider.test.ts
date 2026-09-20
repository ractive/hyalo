import { test, expect } from "bun:test";
import { manifest, evidenceHash, MODEL } from "../src/protocol.ts";
import { ask, boundedFetch } from "../src/provider.ts";
import { fixture, p, response } from "./fixtures.ts";
test("explicit opt-in before dispatch; fixed endpoint and model override environment", async () => {
  const m = fixture(); let calls = 0;
  const fetcher = async (url: string, init?: RequestInit) => {
    calls++; expect(url).toBe("https://api.typesafe.ai/v1/systemone"); expect(init?.redirect).toBe("error");
    const request = JSON.parse(init?.body as string); expect(request.model).toBe(MODEL);
    return Response.json(response(request));
  };
  await expect(ask(m, { allowNetwork: false, apiKey: "test-key", fetch: fetcher })).rejects.toThrow("allow-network");
  await expect(ask(m, { allowNetwork: true, fetch: fetcher })).rejects.toThrow("unavailable");
  expect(calls).toBe(0);
  const names = ["TYPESAFE_BASE_URL", "TYPESAFE_DEFAULT_MODEL", "TYPESAFE_LOG_LEVEL"], old = names.map(n => process.env[n]);
  try {
    process.env.TYPESAFE_BASE_URL = "https://wrong.invalid"; process.env.TYPESAFE_DEFAULT_MODEL = "wrong"; process.env.TYPESAFE_LOG_LEVEL = "debug";
    const result = await ask(m, { allowNetwork: true, apiKey: "test-key", fetch: fetcher });
    expect(result.exitCode).toBe(0); expect(calls).toBe(1); expect(result.transmitted).toBe(1);
  } finally { names.forEach((n, i) => { if (old[i] === undefined) delete process.env[n]; else process.env[n] = old[i]; }); }
});
test("one throttle retry; no auth, validation, server or connection retries", async () => {
  for (const status of [401, 422, 500, 429, 529]) {
    let calls = 0;
    const result = await ask(fixture(), { allowNetwork: true, apiKey: "test-key", fetch: async () => { calls++; return new Response('{"error":"secret body"}', { status, headers: { "retry-after-ms": "1" } }); } });
    expect(calls).toBe(status === 429 || status === 529 ? 2 : 1);
    expect(result.exitCode).toBe(1); expect(JSON.stringify(result)).not.toContain("secret body");
    expect(result.results[0]?.usageUnknown).toBe(true);
  }
  let calls = 0;
  await ask(fixture(), { allowNetwork: true, apiKey: "test-key", fetch: async () => { calls++; throw new Error("private transport details"); } });
  expect(calls).toBe(1);
});
test("deadline cancels long Retry-After without retrying early", async () => {
  let calls = 0; const start = performance.now();
  const result = await ask(fixture(), { allowNetwork: true, apiKey: "test-key", deadlineMs: 40, fetch: async () => { calls++; return new Response(null, { status: 429, headers: { "Retry-After": "99999999" } }); } });
  expect(calls).toBe(1); expect(result.exitCode).toBe(1); expect(performance.now() - start).toBeLessThan(1000);
});
test("caps chunked bodies without Content-Length and cancels slow bodies", async () => {
  let cancelled = false;
  const stream = new ReadableStream<Uint8Array>({ pull(c) { c.enqueue(new Uint8Array(100)); }, cancel() { cancelled = true; } });
  const fetcher = boundedFetch(async () => new Response(stream), () => {}, 150);
  await expect(fetcher("https://api.typesafe.ai/v1/systemone", { method: "POST" })).rejects.toThrow("exceeds");
  expect(cancelled).toBe(true);
  const result = await ask(fixture(), { allowNetwork: true, apiKey: "test-key", deadlineMs: 40, fetch: async () => new Response(new ReadableStream({ start() {}, cancel() { cancelled = true; } })) });
  expect(result.exitCode).toBe(1);
});
test("refuses redirects and invalid responses; retains independent successes", async () => {
  for (const reply of [new Response("", { status: 302 }), new Response("not JSON"), Response.json({ model: "wrong" })]) {
    const result = await ask(fixture(), { allowNetwork: true, apiKey: "test-key", fetch: async () => reply });
    expect(result.exitCode).not.toBe(0); expect(result.results[0]?.decisions.every(d => d.status === "defer")).toBe(true);
  }
  const m = fixture(), next = { ...m.documents[0]!, file: "other.md" };
  const { fingerprint: _, ...evidence } = next;
  next.fingerprint = evidenceHash(evidence, p, m.contextHash); m.documents.push(next); m.selected = 2;
  let calls = 0;
  const result = await ask(manifest(m), { allowNetwork: true, apiKey: "test-key", fetch: async (_, init) => ++calls === 1 ? Response.json(response(JSON.parse(init?.body as string))) : new Response("", { status: 401 }) });
  expect(result.results.map(r => r.status)).toEqual(["ok", "unavailable"]); expect(result.exitCode).toBe(1);
});

test("existing type cannot inherit an unapproved folder from Object.prototype", async () => {
  const m = fixture();
  const doc = { ...m.documents[0]!, current: { type: "constructor" }, missingType: false };
  const { fingerprint: _, ...evidence } = doc;
  doc.fingerprint = evidenceHash(evidence, m.policy, m.contextHash); m.documents = [doc];
  const result = await ask(manifest(m), { allowNetwork: true, apiKey: "test-key", fetch: async (_, init) => Response.json(response(JSON.parse(init?.body as string))) });
  expect(result.results[0]?.decisions.some(d => d.field === "folder")).toBe(false);
});
