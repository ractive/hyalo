import { TypeSafeClient, APIError, RateLimitError } from "@typesafe-ai/sdk";
import { decisions, type Decision } from "./decisions.ts";
import { type Manifest, MODEL, LIMITS, Invalid, Unavailable, insist, hash, payload, normalizedType } from "./protocol.ts";

const ENDPOINT = "https://api.typesafe.ai/v1/systemone";
type Fetch = (input: string, init?: RequestInit) => Promise<Response>;

// Bound the stream before the SDK buffers/parses it, including error bodies.
export function boundedFetch(fetcher: Fetch, onDispatch: () => void, maxBytes: number = LIMITS.response): Fetch {
  return async (input, init) => {
    insist(input === ENDPOINT && init?.method === "POST", "unexpected provider endpoint");
    onDispatch();
    const response = await fetcher(input, { ...init, redirect: "error" });
    insist(!response.redirected && !(response.status >= 300 && response.status < 400), "provider redirect refused");
    const headers = new Headers(response.headers);
    const retryAfter = new RateLimitError(response.status, undefined, headers).retryAfterMs;
    if ((response.status === 429 || response.status === 529) && retryAfter !== undefined && retryAfter > LIMITS.deadline) {
      // JS timers overflow above 2^31-1 and retry immediately. Normalize to
      // beyond our total budget: cancellation wins, with no early retry.
      headers.delete("retry-after"); headers.set("retry-after-ms", String(LIMITS.deadline + 1));
    }
    if (!response.body) return new Response(null, { status: response.status, headers });
    const reader = response.body.getReader(), chunks: Uint8Array[] = [];
    let bytes = 0;
    const cancel = () => { void reader.cancel().catch(() => {}); };
    init.signal?.addEventListener("abort", cancel, { once: true });
    try {
      init.signal?.throwIfAborted();
      while (true) {
        const part = await reader.read();
        init.signal?.throwIfAborted();
        if (part.done) break;
        bytes += part.value.byteLength;
        insist(bytes <= maxBytes, "provider response exceeds limit"); chunks.push(part.value);
      }
      return new Response(Buffer.concat(chunks), { status: response.status, statusText: response.statusText, headers });
    } finally {
      init.signal?.removeEventListener("abort", cancel);
      void reader.cancel().catch(() => {});
      reader.releaseLock();
    }
  };
}

export interface BatchResult {
  file: string; fingerprint: string; requestHash: string | null; status: "ok" | "unavailable" | "invalid";
  decisions: Decision[]; attempts: number; elapsedMs: number;
  usage: { input_tokens: number; output_tokens: number } | null; usageUnknown: boolean; estimatedCostUsd: number | null;
  reason?: string;
}
export async function ask(m: Manifest, options: { allowNetwork: boolean; apiKey?: string; fetch?: Fetch; deadlineMs?: number }) {
  insist(options.allowNetwork, "ask requires explicit --allow-network");
  const key = options.apiKey?.trim();
  if (!key) throw new Unavailable("TYPESAFE_API_KEY is unavailable; continue ordinary tidy");
  // Prevent the active credential from being copied to an API state by mistake.
  insist(!JSON.stringify(m).includes(key), "credential appears in manifest");
  const results: BatchResult[] = [];
  for (const doc of m.documents) {
    const { request, bindings } = payload(doc, m.policy), started = performance.now();
    let attempts = 0;
    let transportInvalid = false;
    const hasQuestions = Object.keys(request.questions).length > 0;
    const row: BatchResult = { file: doc.file, fingerprint: doc.fingerprint, requestHash: hasQuestions ? hash(request) : null, status: "ok", decisions: [], attempts: 0, elapsedMs: 0, usage: { input_tokens: 0, output_tokens: 0 }, usageUnknown: false, estimatedCostUsd: 0 };
    try {
      if (hasQuestions) {
        const deadline = options.deadlineMs ?? LIMITS.deadline;
        insist(Number.isSafeInteger(deadline) && deadline > 0 && deadline <= LIMITS.deadline, "invalid request deadline");
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), deadline);
        const transport = boundedFetch(options.fetch ?? fetch, () => { attempts++; });
        try {
          const client = new TypeSafeClient({
            apiKey: key, baseURL: "https://api.typesafe.ai", defaultModel: MODEL, logLevel: "off", timeout: deadline,
            retry: { maxRetries: 1, httpStatuses: new Set([429, 529]), apiConnectionError: false, apiTimeoutError: false, respectRetryAfter: true, maxRetryAfterMs: LIMITS.deadline + 1, backoffInitialMs: 250, backoffMaxMs: 250, backoffJitter: 0 },
            fetch: async (input, init) => { try { return await transport(input, init); } catch (error) { if (error instanceof Invalid) transportInvalid = true; throw error; } },
          });
          const response: unknown = await client.systemOne(request, { signal: controller.signal });
          const validated = decisions(response, request.questions, bindings);
          row.decisions = validated.decisions; row.usage = validated.usage;
          row.estimatedCostUsd = validated.usage.input_tokens * .042 / 1_000_000;
          // A successful retry does not account for possible usage of its first attempt.
          row.usageUnknown = attempts > 1;
        } finally { clearTimeout(timer); }
      }
    } catch (error) {
      row.status = error instanceof Invalid || transportInvalid ? "invalid" : "unavailable";
      // Never serialize SDK errors: their messages can contain provider bodies.
      row.reason = row.status === "invalid" ? "invalid provider response" : error instanceof APIError ? `provider HTTP ${error.status}` : "provider unavailable or deadline exceeded";
      row.usage = null; row.estimatedCostUsd = null; row.usageUnknown = attempts > 0;
      row.decisions = Object.values(bindings).map(b => ({ field: b.field, status: "defer", reason: row.reason }));
    }
    // Existing local filing conventions remain usable when independent tag
    // questions fail. A failed classification never supplies a missing type.
    const type = row.decisions.find(d => d.field === "type" && d.status === "suggestion")?.value ?? normalizedType(doc.current.type);
    const mapped = type && Object.hasOwn(m.policy.typeFolders, type) ? m.policy.typeFolders[type] : undefined;
    if (mapped) row.decisions.push({ field: "folder", status: "suggestion", value: mapped, reason: "local type-to-folder convention" });
    for (const d of row.decisions) if (d.field === "folder" && d.value === doc.file.split("/").slice(0, -1).join("/")) d.status = "no-change";
    if (!doc.missingType && !("type" in doc.current) && m.policy.types.length) row.decisions.push({ field: "type", status: "defer", reason: "type eligibility not established by Hyalo" });
    row.attempts = attempts; row.elapsedMs = Math.round(performance.now() - started); results.push(row);
  }
  const exitCode = results.some(r => r.status === "invalid") ? 2 : results.some(r => r.status === "unavailable") ? 1 : 0;
  return { version: 1, model: MODEL, selected: m.selected, transmitted: results.filter(r => r.attempts > 0).length, deferred: m.deferred, results, exitCode };
}
