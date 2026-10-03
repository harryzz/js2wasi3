// node's global `fetch` (T72; undici in node) over `wasi:http@0.3.0`'s `client.send`: the host makes the
// connection, does HTTPS and HTTP/1.1–2. Written from the Fetch standard's main fetch / HTTP fetch /
// HTTP-redirect fetch steps with undici's defaults, headers and errors (`TypeError: fetch failed` with
// node's error as `cause`). Apart from `node:internal/fetch` so programs that only use `Headers` or
// `Response` don't import `wasi:http`.
"use strict";

const { Headers, Request, Response, ReadableStream, kState, kAppend } = require("node:internal/fetch");
const { URL } = require("node:internal/url");
const { statusCodes } = require("node:internal/builtin_data");
const wasi = require("node:internal/wasi_http");

// What undici adds when the request doesn't have them (no `accept-encoding`: responses aren't decoded).
const DEFAULT_HEADERS = [["accept", "*/*"], ["accept-language", "*"], ["sec-fetch-mode", "cors"], ["user-agent", "node"]];
// `wasmtime`'s forbidden request headers (`DEFAULT_FORBIDDEN_HEADERS`): the host sets them.
const FORBIDDEN = new Set(["connection", "keep-alive", "proxy-authenticate", "proxy-authorization",
  "proxy-connection", "transfer-encoding", "upgrade", "host", "http2-settings"]);
const NULL_BODY = new Set([101, 103, 204, 205, 304]);
const REDIRECT = new Set([301, 302, 303, 307, 308]);
const METHODS = { GET: "get", HEAD: "head", POST: "post", PUT: "put", DELETE: "delete", CONNECT: "connect",
  OPTIONS: "options", TRACE: "trace", PATCH: "patch" };
const OK = { tag: "ok", val: undefined };

function fetchFailed(cause) {
  return new TypeError("fetch failed", { cause });
}

function nodeError(message, props) {
  return Object.assign(new Error(message), props);
}

// node's error for a `wasi:http` `error-code` (what undici's `cause` would be).
// `body`: the error came with the response body (hyper's protocol error for a body cut short).
function causeOf(code, url, body = false) {
  const tag = code !== null && typeof code === "object" ? code.tag : String(code);
  if (body && tag === "HTTP-protocol-error") return causeOf("connection-terminated", url);
  const host = url.hostname.startsWith("[") ? url.hostname.slice(1, -1) : url.hostname;
  const port = Number(url.port || (url.protocol === "https:" ? 443 : 80));
  switch (tag) {
    case "connection-refused":
      return nodeError(`connect ECONNREFUSED ${host}:${port}`,
        { errno: -111, code: "ECONNREFUSED", syscall: "connect", address: host, port });
    case "DNS-error":
    case "destination-not-found":
      return nodeError(`getaddrinfo ENOTFOUND ${host}`,
        { errno: -3008, code: "ENOTFOUND", syscall: "getaddrinfo", hostname: host });
    case "DNS-timeout":
      return nodeError(`getaddrinfo EAI_AGAIN ${host}`,
        { errno: -3001, code: "EAI_AGAIN", syscall: "getaddrinfo", hostname: host });
    case "destination-unavailable":
    case "destination-IP-unroutable":
      return nodeError(`connect EHOSTUNREACH ${host}:${port}`,
        { errno: -113, code: "EHOSTUNREACH", syscall: "connect", address: host, port });
    case "connection-timeout":
      return nodeError(`Connect Timeout Error (attempted address: ${host}:${port}, timeout: 10000ms)`,
        { name: "ConnectTimeoutError", code: "UND_ERR_CONNECT_TIMEOUT" });
    case "connection-terminated":
    case "HTTP-response-incomplete":
      return nodeError("other side closed", { name: "SocketError", code: "UND_ERR_SOCKET" });
    case "HTTP-response-timeout":
    case "connection-read-timeout":
      return nodeError("Headers Timeout Error", { name: "HeadersTimeoutError", code: "UND_ERR_HEADERS_TIMEOUT" });
    case "TLS-certificate-error":
      return nodeError("unable to verify the first certificate", { code: "UNABLE_TO_VERIFY_LEAF_SIGNATURE" });
    case "TLS-protocol-error":
    case "TLS-alert-received":
      return nodeError(`TLS error (${tag})`, { code: "ERR_SSL_PROTOCOL_ERROR" });
    default: {
      const val = code !== null && typeof code === "object" && code.val !== undefined ? `: ${JSON.stringify(code.val)}` : "";
      return nodeError(`${tag}${val} (wasi:http)`, { code: "UND_ERR" });
    }
  }
}

// A `wasi:http` field value (bytes) as a ByteString, and back.
function latin1(bytes) {
  let s = "";
  for (let i = 0; i < bytes.length; i++) s += String.fromCharCode(bytes[i]);
  return s;
}

function bytesOf(s) {
  const b = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) b[i] = s.charCodeAt(i) & 0xff;
  return b;
}

function abortReason(signal) {
  return signal.reason;
}

// One request/response exchange: resolves to `[handle, status, headers]` (the response handle's
// body not yet consumed), rejects with node's error.
function sendOnce(url, method, headers, body, signal) {
  const list = [];
  for (const [name, value] of headers) if (!FORBIDDEN.has(name)) list.push([name, bytesOf(value)]);
  const fields = wasi.fromList(list);
  const contents = body === null ? undefined : body instanceof Uint8Array ? [body] : body;
  const [req] = wasi.newRequest(fields, contents, OK, undefined);
  const m = METHODS[method];
  wasi.setMethod(req, m !== undefined ? { tag: m } : { tag: "other", val: method });
  wasi.setScheme(req, { tag: url.protocol === "https:" ? "HTTPS" : "HTTP" });
  wasi.setAuthority(req, url.host);
  wasi.setPathWithQuery(req, url.pathname + url.search);
  const sent = wasi.send(req);
  return new Promise((resolve, reject) => {
    let onAbort;
    if (signal) {
      onAbort = () => {
        wasi.cancel(sent);
        reject(abortReason(signal));
      };
      signal.addEventListener("abort", onAbort, { once: true });
    }
    sent.then(
      (res) => {
        if (onAbort) signal.removeEventListener("abort", onAbort);
        const h = wasi.getHeaders(res);
        const entries = wasi.copyAll(h);
        wasi.dropFields(h);
        resolve([res, wasi.getStatusCode(res), entries]);
      },
      (e) => {
        if (onAbort) signal.removeEventListener("abort", onAbort);
        reject(fetchFailed(causeOf(e && "payload" in e ? e.payload : e, url)));
      },
    );
  });
}

// The response body as a `ReadableStream`: `consume-body` on the first read (an unread body keeps
// nothing waiting, as node exits with one), errors with undici's `terminated` if the host reports one.
function bodyOf(res, url, signal) {
  let chunks = null;
  let trailers = null;
  let onAbort;
  const stream = new ReadableStream({
    async pull(c) {
      if (chunks === null) {
        const [body, t] = wasi.consumeBody(res, OK);
        chunks = body;
        trailers = t;
      }
      const { value, done } = await chunks.next();
      if (!done) {
        c.enqueue(value);
        return;
      }
      if (onAbort) signal.removeEventListener("abort", onAbort);
      const r = await trailers;
      if (r && r.tag === "err") c.error(new TypeError("terminated", { cause: causeOf(r.val, url, true) }));
      else c.close();
    },
    cancel() {
      if (onAbort) signal.removeEventListener("abort", onAbort);
      if (chunks !== null) {
        chunks.cancel();
        chunks.return();
      } else {
        wasi.dropResponse(res);
      }
    },
  }, { highWaterMark: 0 });
  if (signal) {
    onAbort = () => {
      if (chunks !== null) {
        chunks.cancel();
        chunks.return();
      }
      stream._error(abortReason(signal));
    };
    signal.addEventListener("abort", onAbort, { once: true });
  }
  return stream;
}

async function fetch(input, init = undefined) {
  if (arguments.length === 0) {
    throw new TypeError("fetch: 1 argument required, but only 0 present.");
  }
  const request = new Request(input, init);
  const st = request[kState];
  const redirectMode = st.redirect;
  if (redirectMode !== "follow" && redirectMode !== "manual" && redirectMode !== "error") {
    throw new TypeError(`undefined: ${redirectMode} is not an accepted type. Expected one of follow, manual, error.`);
  }
  const signal = st.signal;
  if (signal && signal.aborted) throw abortReason(signal);
  let url = new URL(st.url);
  url.hash = "";
  let method = st.method;
  let body = st.body;
  if (body !== null && !(body instanceof Uint8Array)) st.body = null;
  // undici's headers after the request's own.
  const headers = new Headers();
  for (const [name, value] of st.headers) headers[kAppend](name, value);
  for (const [name, value] of DEFAULT_HEADERS) if (!headers.has(name)) headers[kAppend](name, value);
  const urls = [url.href];
  for (;;) {
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      throw fetchFailed(new Error(urls.length > 1 ? "URL scheme must be a HTTP(S) scheme" : "unknown scheme"));
    }
    const sendHeaders = new Headers();
    for (const [name, value] of headers) sendHeaders[kAppend](name, value);
    if (body instanceof Uint8Array) sendHeaders[kAppend]("content-length", String(body.length));
    else if (body === null && (method === "POST" || method === "PUT")) sendHeaders[kAppend]("content-length", "0");
    const [res, status, entries] = await sendOnce(url, method, sendHeaders, body, signal);
    const location = REDIRECT.has(status) ? entries.find(([k]) => k === "location") : undefined;
    if (location !== undefined && redirectMode !== "manual") {
      if (redirectMode === "error") {
        wasi.dropResponse(res);
        throw fetchFailed(new Error("unexpected redirect"));
      }
      let next;
      try {
        next = new URL(latin1(location[1]), url);
      } catch (e) {
        wasi.dropResponse(res);
        throw fetchFailed(e);
      }
      wasi.dropResponse(res);
      if (urls.length > 20) throw fetchFailed(new Error("redirect count exceeded"));
      if (body !== null && !(body instanceof Uint8Array) && status !== 303 && !((status === 301 || status === 302) && method === "POST")) {
        throw fetchFailed(new Error("Cannot follow redirect with a streamed body"));
      }
      if ((status === 303 && method !== "GET" && method !== "HEAD") || ((status === 301 || status === 302) && method === "POST")) {
        method = "GET";
        body = null;
        for (const name of ["content-encoding", "content-language", "content-location", "content-type", "content-length"]) {
          headers.delete(name);
        }
      }
      if (next.origin !== url.origin) {
        for (const name of ["authorization", "cookie", "proxy-authorization", "host"]) headers.delete(name);
      }
      url = next;
      url.hash = "";
      urls.push(url.href);
      continue;
    }
    const response = new Response(null, { status: 200 });
    const rs = response[kState];
    rs.type = "basic";
    rs.url = url.href;
    rs.redirected = urls.length > 1;
    rs.status = status;
    rs.statusText = statusCodes[status] || "";
    for (const [name, value] of entries) rs.headers[kAppend](name, latin1(value));
    if (method === "HEAD" || method === "CONNECT" || NULL_BODY.has(status)) wasi.dropResponse(res);
    else rs.body = bodyOf(res, url, signal);
    return response;
  }
}

if (globalThis.fetch === undefined) {
  Object.defineProperty(globalThis, "fetch", { value: fetch, writable: true, enumerable: true, configurable: true });
}

module.exports = { fetch };
