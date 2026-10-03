// `fetch` (T72) against an `http` server in the same program, each step awaited. The server prints
// what it got; headers the HTTP stack owns (`connection`, `keep-alive`, `date`) aren't printed.
import http from "node:http";

const skip = new Set(["connection", "keep-alive", "date", "accept-encoding", "transfer-encoding"]);
const sortedHeaders = (raw) => {
  const out = [];
  for (let i = 0; i < raw.length; i += 2) if (!skip.has(raw[i].toLowerCase())) out.push(`${raw[i].toLowerCase()}=${raw[i + 1].replace(/:\d+$/, ":<port>")}`);
  return out.sort().join(" ");
};
const shown = (headers) => [...headers].filter(([k]) => !skip.has(k)).map(([k, v]) => `${k}=${v}`).join(" ");

const server = http.createServer((req, res) => {
  let body = "";
  req.setEncoding("utf8");
  req.on("data", (d) => (body += d));
  req.on("end", () => {
    console.log("server:", req.method, req.url, sortedHeaders(req.rawHeaders), JSON.stringify(body));
    const u = new URL(req.url, "http://x");
    switch (u.pathname) {
      case "/json":
        res.setHeader("content-type", "application/json");
        res.setHeader("x-reply", "yes");
        res.end(JSON.stringify({ got: body ? JSON.parse(body) : null, q: u.searchParams.get("q") }));
        break;
      case "/missing":
        res.writeHead(404, { "content-type": "text/plain" });
        res.end("no such thing");
        break;
      case "/r1":
        res.writeHead(302, { location: "/r2" });
        res.end();
        break;
      case "/r2":
        res.writeHead(301, { location: "http://127.0.0.1:" + port + "/echo?from=r2" });
        res.end();
        break;
      case "/see-other":
        res.writeHead(303, { location: "/echo" });
        res.end();
        break;
      case "/temp":
        res.writeHead(307, { location: "/echo" });
        res.end();
        break;
      case "/loop":
        res.writeHead(302, { location: "/loop" });
        res.end();
        break;
      case "/chunks":
        res.write("one,");
        setTimeout(() => res.write("two,"), 20);
        setTimeout(() => res.end("three"), 40);
        break;
      case "/slow":
        setTimeout(() => res.end("late"), 400);
        break;
      case "/partial":
        res.writeHead(200, { "content-length": "100" });
        res.write("only part");
        setTimeout(() => res.destroy(), 50);
        break;
      case "/stall":
        res.write("first");
        setTimeout(() => res.end("rest"), 400);
        break;
      case "/empty":
        res.writeHead(204);
        res.end();
        break;
      default:
        res.setHeader("content-type", "text/plain");
        res.end(`${req.method} ${req.url} ${body}`);
    }
  });
});

let port;
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
port = server.address().port;
const base = `http://127.0.0.1:${port}`;
const short = (u) => u.replace(base, "<base>");

async function step(name, f) {
  try {
    await f();
  } catch (e) {
    console.log(name, "threw", e.name, e.message, e.cause ? `cause: ${e.cause.name} ${e.cause.code} ${short(e.cause.message)}` : "");
  }
}

await step("get", async () => {
  const r = await fetch(`${base}/plain?a=1#frag`);
  console.log("get:", r.status, r.statusText, r.ok, r.type, short(r.url), r.redirected, r.bodyUsed, shown(r.headers));
  console.log("get text:", JSON.stringify(await r.text()), r.bodyUsed);
});

await step("json", async () => {
  const r = await fetch(`${base}/json?q=x%20y`, {
    method: "POST",
    headers: { "Content-Type": "application/json", "X-Token": "abc" },
    body: JSON.stringify({ n: 1, s: "é" }),
  });
  console.log("json:", r.status, r.headers.get("x-reply"), r.headers.get("content-type"), JSON.stringify(await r.json()));
});

await step("missing", async () => {
  const r = await fetch(`${base}/missing`);
  console.log("missing:", r.status, r.statusText, r.ok, await r.text());
});

await step("redirects", async () => {
  const r = await fetch(`${base}/r1`, { method: "POST", body: "dropped" });
  console.log("redirects:", r.status, r.redirected, short(r.url), await r.text());
});

await step("see-other", async () => {
  const r = await fetch(`${base}/see-other`, { method: "PUT", body: "dropped too", headers: { "content-type": "text/x" } });
  console.log("see-other:", r.status, short(r.url), await r.text());
});

await step("temporary", async () => {
  const r = await fetch(`${base}/temp`, { method: "POST", body: "kept" });
  console.log("temporary:", r.status, await r.text());
});

await step("manual", async () => {
  const r = await fetch(`${base}/r1`, { redirect: "manual" });
  console.log("manual:", r.status, r.statusText, r.headers.get("location"), r.redirected, short(r.url));
});

await step("redirect error", () => fetch(`${base}/r1`, { redirect: "error" }));
await step("redirect loop", () => fetch(`${base}/loop`));

await step("chunks", async () => {
  const r = await fetch(`${base}/chunks`);
  const reader = r.body.getReader();
  let text = "";
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    console.log("chunk is Uint8Array:", value instanceof Uint8Array);
    text += new TextDecoder().decode(value);
  }
  console.log("chunks:", text, r.bodyUsed);
});

await step("bytes", async () => {
  const r = await fetch(`${base}/bytes`, { method: "POST", body: new Uint8Array([104, 105]) });
  const buf = Buffer.from(await r.arrayBuffer());
  console.log("bytes:", buf.length, buf.toString());
  await r.text();
});

await step("used twice", async () => {
  const r = await fetch(`${base}/twice`);
  await r.text();
  await r.text();
});

await step("form", async () => {
  const r = await fetch(`${base}/form`, { method: "POST", body: new URLSearchParams({ a: "1", b: "x y" }) });
  console.log("form:", await r.text());
});

await step("stream body", async () => {
  const body = new ReadableStream({
    start(c) {
      c.enqueue(new TextEncoder().encode("streamed "));
      c.enqueue(new TextEncoder().encode("body"));
      c.close();
    },
  });
  const r = await fetch(`${base}/stream`, { method: "POST", body, duplex: "half" });
  console.log("stream body:", await r.text());
});

await step("no duplex", () => fetch(`${base}/x`, { method: "POST", body: new ReadableStream() }));

await step("request object", async () => {
  const req = new Request(`${base}/req`, { method: "PATCH", headers: { "x-from": "request" }, body: "patched" });
  const r = await fetch(req);
  console.log("request object:", await r.text());
});

await step("head", async () => {
  const r = await fetch(`${base}/plain`, { method: "HEAD" });
  console.log("head:", r.status, r.body, r.headers.get("content-length"));
});

await step("no content", async () => {
  const r = await fetch(`${base}/empty`);
  console.log("no content:", r.status, r.body, JSON.stringify(await r.text()));
});

await step("unread", async () => {
  const r = await fetch(`${base}/plain`);
  console.log("unread:", r.status);
});

await step("aborted before", async () => {
  const ac = new AbortController();
  ac.abort();
  await fetch(`${base}/plain`, { signal: ac.signal });
});

await step("aborted while waiting", async () => {
  const ac = new AbortController();
  setTimeout(() => ac.abort(), 50);
  await fetch(`${base}/slow`, { signal: ac.signal });
});

await step("timeout reason", async () => {
  await fetch(`${base}/slow`, { signal: AbortSignal.timeout(50) });
});

await step("aborted while reading", async () => {
  const ac = new AbortController();
  const r = await fetch(`${base}/stall`, { signal: ac.signal });
  const reader = r.body.getReader();
  const first = await reader.read();
  console.log("first:", new TextDecoder().decode(first.value));
  ac.abort(new Error("enough"));
  await reader.read();
});

await step("cut off", async () => {
  const r = await fetch(`${base}/partial`);
  console.log("cut off:", r.status);
  await r.text();
});

await step("refused", async () => {
  const s = http.createServer();
  await new Promise((resolve) => s.listen(0, "127.0.0.1", resolve));
  const free = s.address().port;
  await new Promise((resolve) => s.close(resolve));
  try {
    await fetch(`http://127.0.0.1:${free}/`);
  } catch (e) {
    console.log("refused:", e.name, e.message, e.cause.code, e.cause.errno, e.cause.syscall, e.cause.address, e.cause.port === free);
  }
});

await step("bad url", () => fetch("not a url"));
await step("bad scheme", () => fetch("ftp://example.com/"));
await step("get with body", () => fetch(`${base}/x`, { body: "no" }));
await step("bad method", () => fetch(`${base}/x`, { method: "CONNECT" }));

console.log(typeof fetch, typeof globalThis.fetch, globalThis.fetch === fetch);
server.close(() => console.log("closed"));
