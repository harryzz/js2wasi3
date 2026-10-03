// node's web globals Headers, Request, Response, ReadableStream, Blob (T71: node:internal/fetch, for srvx and
// T72's fetch). Body reads are awaited one by one (their microtask counts differ from undici's).
const tryit = (f) => {
  try {
    console.log("ok", f());
  } catch (e) {
    console.log(e.name, e.code, e.message);
  }
};
tryit(() => new Headers({ "bad name": "x" }));
tryit(() => new Headers({ a: "x\ny" }));
tryit(() => new Headers([["a"]]));
tryit(() => new Headers(5));
tryit(() => new Headers().get("bad name"));
tryit(() => new Headers().set("a b", "1"));
const h = new Headers([["B", " 2 "], ["a", "1"], ["set-cookie", "x=1"], ["Set-Cookie", "y=2"], ["a", "3"]]);
console.log([...h], h.get("A"), h.getSetCookie(), h.has("b"), [...h.keys()], String(h.entries()), Object.prototype.toString.call(h));
h.delete("a");
h.set("c", "4");
h.forEach((v, k, o) => console.log(k, v, o === h));
console.log([...new Headers(h)], [...new Headers({ z: 1, y: "two" }).values()], new Headers().get("missing"));

const r = new Response("hi", { status: 201, statusText: "Made", headers: { X: "1" } });
console.log(r.status, r.statusText, r.ok, r.headers.get("x"), r.headers.get("content-type"), r.bodyUsed, r.type, r.url,
  r.redirected, typeof r.body, r instanceof Response, Object.prototype.toString.call(r));
console.log("text", await r.text(), r.bodyUsed);
await r.text().catch((e) => console.log(e.name, e.message));
tryit(() => new Response(null, { status: 99 }));
tryit(() => new Response("x", { status: 204 }));
const r2 = new Response(null, { status: 404 });
console.log(r2.status, r2.statusText, r2.ok, r2.body, [...r2.headers]);
console.log("json", await Response.json({ a: 1 }).json(), Response.json([1], { status: 202 }).status);
const ab = await new Response(new Uint8Array([104, 105])).arrayBuffer();
console.log(ab.byteLength, Object.prototype.toString.call(ab));
console.log("bytes", await new Response("abc").bytes());
const e = Response.error();
console.log(e.type, e.status, e.ok);
const red = Response.redirect("http://x.y/z", 301);
console.log(red.status, red.headers.get("location"));
const cl = new Response("clone me");
const cl2 = cl.clone();
console.log(await cl.text(), await cl2.text());

const q = new Request("http://a.b/c?d", { method: "post", headers: { A: "1" }, body: "zz" });
console.log(q.method, q.url, q.headers.get("a"), q.headers.get("content-type"), q.bodyUsed, q.redirect, q.mode,
  q.credentials, q.cache, q.duplex, q.signal.aborted, Object.prototype.toString.call(q));
console.log("req text", await q.text());
console.log(new Request("http://a/", { method: "patch" }).method, new Request("http://a/", { method: "delete" }).method);
tryit(() => new Request("/rel"));
tryit(() => new Request("http://a/", { method: "GET", body: "x" }));
tryit(() => new Request("http://a/", { method: "CONNECT" }));
tryit(() => new Request("http://a/", { method: "bad method" }));

// ReadableStream: a source that enqueues, a reader, async iteration.
const rs = new ReadableStream({
  start(c) {
    c.enqueue(new TextEncoder().encode("one "));
  },
  pull(c) {
    if (!this.n) {
      this.n = 1;
      c.enqueue(new TextEncoder().encode("two"));
    } else {
      c.close();
    }
  },
});
console.log(rs.locked, rs instanceof ReadableStream, Object.prototype.toString.call(rs));
console.log("stream body", await new Response(rs).text());
const rs2 = new ReadableStream({
  start(c) {
    c.enqueue("a");
    c.enqueue("b");
    c.close();
  },
});
const chunks = [];
for await (const c of rs2) chunks.push(c);
console.log(chunks, rs2.locked);
const reader = new ReadableStream({ start(c) { c.error(new Error("boom")); } }).getReader();
await reader.read().catch((err) => console.log("read error", err.message));

const b = new Blob(["ab", new Uint8Array([99])], { type: "Text/Plain" });
console.log(b.size, b.type, await b.text(), await b.slice(1).text(), b instanceof Blob);

console.log(typeof ReadableStream, typeof Blob, Object.getOwnPropertyNames(Headers.prototype).join(" "));
console.log(Object.getOwnPropertyNames(Response.prototype).join(" "));
console.log(Object.getOwnPropertyNames(Request.prototype).join(" "));
console.log(Object.getOwnPropertyNames(Response).join(" "), typeof globalThis.Headers, globalThis.Response === Response);
