import type { Questions } from "@typesafe-ai/sdk";
import { MODEL, type Binding, insist, object, keys } from "./protocol.ts";

export interface Decision { field: "type" | "folder" | "tag"; status: "suggestion" | "defer" | "no-change"; value?: string; confidence?: number; probability?: number; reason?: string }
function probability(value: unknown): number {
  insist(typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= 1, "invalid probability");
  return value;
}
export function decisions(value: unknown, questions: Questions, bindings: Record<string, Binding>) {
  const response = object(value); keys(response, ["model", "answers", "usage"]);
  insist(response.model === MODEL, "unexpected response model");
  const answers = object(response.answers), usage = object(response.usage);
  keys(usage, ["input_tokens", "output_tokens"]);
  for (const field of ["input_tokens", "output_tokens"]) insist(Number.isSafeInteger(usage[field]) && (usage[field] as number) >= 0, "invalid usage");
  insist(Object.keys(answers).length === Object.keys(questions).length && Object.keys(questions).every(id => Object.hasOwn(answers, id)), "answer IDs differ from request");
  const results: Decision[] = Object.entries(questions).map(([id, q]) => {
    const answer = object(answers[id]), binding = bindings[id];
    insist(binding && answer.type === q.type, "answer type differs from request");
    if (q.type === "noul") {
      keys(answer, ["type", "noul"]);
      const p = probability(answer.noul);
      return { field: binding.field, value: binding.value, probability: p, status: p >= .9 ? "suggestion" : p <= .1 ? "no-change" : "defer", ...(p > .1 && p < .9 ? { reason: "uncertain" } : {}) };
    }
    insist(q.type === "choice", "unsupported question type");
    keys(answer, ["type", "choice", "confidence", "probabilities"]);
    const probabilities = object(answer.probabilities), choices = Object.keys(q.criteria);
    insist(Object.keys(probabilities).length === choices.length && choices.every(c => Object.hasOwn(probabilities, c)), "probability labels differ from request");
    const values = choices.map(c => probability(probabilities[c]));
    insist(Math.abs(values.reduce((a, b) => a + b, 0) - 1) <= .001, "probabilities do not sum to one");
    insist(typeof answer.choice === "string" && choices.includes(answer.choice), "unknown selected label");
    const p = probability(probabilities[answer.choice]), confidence = probability(answer.confidence);
    insist(p >= Math.max(...values) - 1e-9, "selected label is not the winner");
    const result = binding.options[answer.choice];
    const accepted = result !== undefined && confidence >= .85 && p >= .9;
    return { field: binding.field, status: accepted ? "suggestion" : "defer", ...(accepted ? { value: result } : { reason: result === undefined ? "unknown" : "uncertain" }), confidence, probability: p };
  });
  return { decisions: results, usage: { input_tokens: usage.input_tokens as number, output_tokens: usage.output_tokens as number } };
}
