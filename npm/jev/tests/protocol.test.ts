import { test, expect } from "bun:test";
import { policy, manifest, payload, hash } from "../src/protocol.ts";
import { selection } from "../src/prepare.ts";
import { decisions } from "../src/decisions.ts";
import { fixture, p, response } from "./fixtures.ts";
test("canonical hashes and local-only metadata", () => {
  expect(hash({ b: 1, a: 2 })).toBe(hash({ a: 2, b: 1 }));
  const m = fixture(), { request } = payload(m.documents[0]!, p);
  expect(Object.keys(request).sort()).toEqual(["model", "questions", "state"]);
  expect(JSON.stringify(request)).not.toContain("private");
  expect(() => manifest({ ...m, documents: [{ ...m.documents[0], content: "changed" }] })).toThrow("fingerprint");
  expect(() => policy({ version: 1, types: p.types, consent: true })).toThrow("unknown");
  expect(() => selection(["../escape.md"])).toThrow();
  expect(() => selection(Array(26).fill("a.md"))).toThrow();
});
test("rejects malformed answers; unknown and uncertain choices defer", () => {
  const { request, bindings } = payload(fixture().documents[0]!, p), valid = response(request);
  expect(decisions(valid, request.questions, bindings).decisions[0]?.status).toBe("suggestion");
  for (const overrides of [{ model: "jev-latest" }, { answers: {} }, { usage: { input_tokens: -1, output_tokens: 1 } }]) expect(() => decisions({ ...valid, ...overrides }, request.questions, bindings)).toThrow();
  for (const patch of [
    { choice: "intruder" }, { type: "score" }, { confidence: Infinity },
    { probabilities: { c0: .1, c1: .89, unknown: .01 } },
    { probabilities: { c0: .9, c1: .9, unknown: .1 } },
    { probabilities: { c0: .99, c1: .01, other: 0 } },
  ]) expect(() => decisions({ ...valid, answers: { ...valid.answers, type: { ...valid.answers.type, ...patch } } }, request.questions, bindings)).toThrow();
  const unknown = { type: "choice", choice: "unknown", confidence: .999, probabilities: { c0: .001, c1: .001, unknown: .998 } };
  expect(decisions({ ...valid, answers: { ...valid.answers, type: unknown } }, request.questions, bindings).decisions[0]?.status).toBe("defer");
  expect(decisions({ ...valid, answers: { ...valid.answers, type: { ...valid.answers.type, confidence: .84 } } }, request.questions, bindings).decisions[0]?.status).toBe("defer");
});
test("additive provider fields are ignored while known fields stay validated", () => {
  const { request, bindings } = payload(fixture().documents[0]!, p), valid = response(request);
  const extended = {
    ...valid, request_id: "req_123", usage: { ...valid.usage, cached_tokens: 5 },
    answers: Object.fromEntries(Object.entries(valid.answers).map(([id, a]) => [id, { ...(a as object), explanation: "ignored", nested: { more: true } }])),
  };
  const result = decisions(extended, request.questions, bindings);
  expect(result.decisions.map(d => d.status)).toEqual(["suggestion", "suggestion"]);
  expect(result.usage).toEqual({ input_tokens: 100, output_tokens: 20 });
  // Unknown keys never relax validation of the keys the helper relies on.
  expect(() => decisions({ ...extended, usage: { ...extended.usage, output_tokens: "20" } }, request.questions, bindings)).toThrow("usage");
  expect(() => decisions({ ...extended, answers: { ...extended.answers, type: { ...(extended.answers.type as object), choice: "intruder" } } }, request.questions, bindings)).toThrow();
});
