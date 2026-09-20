import { policy, manifest, payload, hash, evidenceHash, MODEL, type Manifest } from "../src/protocol.ts";
export const p = policy({ version: 1, types: [{ value: "research", description: "Experiments, measurements and dogfooding reports." }, { value: "docs", description: "Instructions for using a product." }], tags: [{ value: "performance", description: "Measures or improves runtime performance." }] });
export function fixture(content = "Measurements show a 20 percent improvement in startup latency."): Manifest {
  const contextHash = hash({});
  const d = { file: "private/note.md", section: null, content, current: { title: "private metadata" }, missingType: true };
  return manifest({ version: 1, policy: p, contextHash, selected: 1, deferred: [], documents: [{ ...d, fingerprint: evidenceHash(d, p, contextHash) }] });
}
export function response(request: ReturnType<typeof payload>["request"], overrides = {}) {
  return { model: MODEL, answers: Object.fromEntries(Object.entries(request.questions).map(([id, q]) => [id, q.type === "noul" ? { type: "noul", noul: .99 } : { type: "choice", choice: "c0", confidence: .99, probabilities: { c0: .98, c1: .01, unknown: .01 } }])), usage: { input_tokens: 100, output_tokens: 20 }, ...overrides };
}
