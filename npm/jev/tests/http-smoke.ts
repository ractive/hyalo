// Compiled separately and executed under each supported runtime. No API key.
import { createServer } from "node:http";
import assert from "node:assert/strict";
import { boundedFetch } from "../src/provider.ts";

const server = createServer((req, res) => {
  if (req.url === "/large") { res.writeHead(200); res.write("x".repeat(512)); res.end("y".repeat(512)); }
  else { res.writeHead(200); res.flushHeaders(); }
});
await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
try {
  const address = server.address(); assert(address && typeof address !== "string");
  const url = `http://127.0.0.1:${address.port}`;
  const large = boundedFetch(async (_, init) => fetch(url + "/large", init), () => {}, 700);
  await assert.rejects(large("https://api.typesafe.ai/v1/systemone", { method: "POST" }), /exceeds/);
  const slow = boundedFetch(async (_, init) => fetch(url + "/slow", init), () => {});
  const abort = new AbortController(), timer = setTimeout(() => abort.abort(), 80);
  try { await assert.rejects(slow("https://api.typesafe.ai/v1/systemone", { method: "POST", signal: abort.signal })); }
  finally { clearTimeout(timer); }
  process.stdout.write("bounded HTTP smoke passed\n");
} finally {
  const closed = new Promise<void>((resolve, reject) => server.close(e => e ? reject(e) : resolve()));
  server.closeAllConnections(); await closed;
}
