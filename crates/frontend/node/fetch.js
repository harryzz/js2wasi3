// node's web globals `Headers`, `Request`, `Response` (T71; undici in node), with a small `ReadableStream`
// and `Blob`: written from the Fetch, Streams and File standards with undici's messages, as far as
// srvx (zigbee2mqtt's frontend) and the T72 `fetch` need them. Loading the module also makes them properties
// of the global object (`globalThis.Headers`), as packages read them there.
"use strict";

const { TextEncoder, TextDecoder } = require("node:internal/encoding");
const { URL, URLSearchParams } = require("node:internal/url");

const encoder = new TextEncoder();
const kGuard = Symbol("guard");
const kAppend = Symbol("append");
const kSorted = Symbol("sorted");
const kRaw = Symbol("raw");
const kState = Symbol("state");

// ---- Headers ----

const tokenRegExp = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
const isToken = (s) => tokenRegExp.test(s);

// The header value normalization: no leading/trailing HTTP whitespace.
function normalizeValue(v) {
  let a = 0;
  let b = v.length;
  const ws = (c) => c === 9 || c === 10 || c === 13 || c === 32;
  while (a < b && ws(v.charCodeAt(a))) a++;
  while (b > a && ws(v.charCodeAt(b - 1))) b--;
  return a === 0 && b === v.length ? v : v.slice(a, b);
}

function byteString(v, context) {
  const s = String(v);
  for (let i = 0; i < s.length; i++) {
    if (s.charCodeAt(i) > 255) {
      throw new TypeError(`${context}: Cannot convert argument to a ByteString because the character at index ${i} has a value of ${s.charCodeAt(i)} which is greater than 255.`);
    }
  }
  return s;
}

function checkName(name, method) {
  if (!isToken(name)) throw new TypeError(`Headers.${method}: "${name}" is an invalid header name.`);
}

function checkValue(value, method) {
  if (value.includes("\0") || value.includes("\r") || value.includes("\n")) {
    throw new TypeError(`Headers.${method}: "${value}" is an invalid header value.`);
  }
}

class Headers {
  #list = new Map(); // lower-case name → { name, value } (set-cookie: values)
  #cookies = [];
  [kGuard] = "none";

  constructor(init = undefined) {
    if (init === undefined || init === null) {
      if (init === null) {
        throw new TypeError("Headers constructor: Argument 1 could not be converted to one of: sequence<sequence<ByteString>>, record<ByteString, ByteString>.");
      }
      return;
    }
    fillHeaders(this, init, "Headers constructor");
  }

  append(name, value) {
    name = byteString(name, "Headers.append");
    value = normalizeValue(byteString(value, "Headers.append"));
    checkName(name, "append");
    checkValue(value, "append");
    this[kAppend](name, value);
  }

  [kAppend](name, value) {
    const key = name.toLowerCase();
    if (key === "set-cookie") this.#cookies.push(value);
    const e = this.#list.get(key);
    if (e) e.value += (key === "cookie" ? "; " : ", ") + value;
    else this.#list.set(key, { name, value });
  }

  delete(name) {
    name = byteString(name, "Headers.delete");
    checkName(name, "delete");
    const key = name.toLowerCase();
    if (key === "set-cookie") this.#cookies = [];
    this.#list.delete(key);
  }

  get(name) {
    name = byteString(name, "Headers.get");
    checkName(name, "get");
    const e = this.#list.get(name.toLowerCase());
    return e === undefined ? null : e.value;
  }

  has(name) {
    name = byteString(name, "Headers.has");
    checkName(name, "has");
    return this.#list.has(name.toLowerCase());
  }

  set(name, value) {
    name = byteString(name, "Headers.set");
    value = normalizeValue(byteString(value, "Headers.set"));
    checkName(name, "set");
    checkValue(value, "set");
    const key = name.toLowerCase();
    if (key === "set-cookie") this.#cookies = [value];
    const e = this.#list.get(key);
    if (e) e.value = value;
    else this.#list.set(key, { name, value });
  }

  getSetCookie() {
    return this.#cookies.slice();
  }

  // The sorted and combined header list (set-cookie values one by one).
  [kSorted]() {
    const out = [];
    for (const key of [...this.#list.keys()].sort()) {
      if (key === "set-cookie") for (const v of this.#cookies) out.push([key, v]);
      else out.push([key, this.#list.get(key).value]);
    }
    return out;
  }

  // Raw (name as first given, value) pairs in insertion order.
  [kRaw]() {
    return [...this.#list.values()].map((e) => [e.name, e.value]);
  }

  keys() {
    return new HeadersIterator(this[kSorted](), 0);
  }

  values() {
    return new HeadersIterator(this[kSorted](), 1);
  }

  entries() {
    return new HeadersIterator(this[kSorted](), 2);
  }

  forEach(callback, thisArg = undefined) {
    if (typeof callback !== "function") {
      throw new TypeError("Headers.forEach: Argument 1 could not be converted to: Function.");
    }
    for (const [k, v] of this[kSorted]()) callback.call(thisArg, v, k, this);
  }

  [Symbol.iterator]() {
    return this.entries();
  }

  get [Symbol.toStringTag]() {
    return "Headers";
  }

  [Symbol.for("nodejs.util.inspect.custom")](depth, options, inspect) {
    const o = {};
    for (const e of this.#list.values()) o[e.name] = e.value;
    return `Headers ${inspect(o, options)}`;
  }
}

function fillHeaders(headers, init, context) {
  if (typeof init !== "object" && typeof init !== "function") {
    throw new TypeError(`${context}: Argument 1 could not be converted to one of: sequence<sequence<ByteString>>, record<ByteString, ByteString>.`);
  }
  if (init instanceof Headers) {
    for (const [k, v] of init[kRaw]()) if (k.toLowerCase() !== "set-cookie") headers[kAppend](k, v);
    for (const v of init.getSetCookie()) headers[kAppend]("set-cookie", v);
    return;
  }
  if (typeof init[Symbol.iterator] === "function") {
    for (const pair of init) {
      const p = typeof pair === "string" ? pair : [...pair];
      if (p.length !== 2) {
        throw new TypeError(`${context}: expected name/value pair to be length 2, found ${p.length}.`);
      }
      headers.append(p[0], p[1]);
    }
    return;
  }
  for (const key of Reflect.ownKeys(init)) {
    const d = Reflect.getOwnPropertyDescriptor(init, key);
    if (d === undefined || !d.enumerable) continue;
    if (typeof key === "symbol") throw new TypeError(`${context}: Key Symbol() in init is a symbol, which cannot be converted to a ByteString.`);
    headers.append(key, init[key]);
  }
}

class HeadersIterator {
  #items;
  #kind;
  #index = 0;

  constructor(items, kind) {
    this.#items = items;
    this.#kind = kind;
  }

  next() {
    if (this.#index >= this.#items.length) return { value: undefined, done: true };
    const [k, v] = this.#items[this.#index++];
    return { value: this.#kind === 0 ? k : this.#kind === 1 ? v : [k, v], done: false };
  }

  [Symbol.iterator]() {
    return this;
  }

  get [Symbol.toStringTag]() {
    return "Headers Iterator";
  }
}

// ---- ReadableStream (default streams; no byte streams, tee or piping) ----

class ReadableStreamDefaultController {
  #stream;

  constructor(stream) {
    this.#stream = stream;
  }

  get desiredSize() {
    const s = this.#stream;
    if (s._state === "errored") return null;
    if (s._state === "closed") return 0;
    return s._highWaterMark - s._queue.length;
  }

  enqueue(chunk) {
    const s = this.#stream;
    if (s._closeRequested || s._state !== "readable") {
      throw new TypeError("Invalid state: Controller is already closed");
    }
    if (s._reads.length > 0) s._reads.shift().resolve({ value: chunk, done: false });
    else s._queue.push(chunk);
    s._pullIfNeeded();
  }

  close() {
    const s = this.#stream;
    if (s._closeRequested || s._state !== "readable") {
      throw new TypeError("Invalid state: Controller is already closed");
    }
    s._closeRequested = true;
    if (s._queue.length === 0) s._close();
  }

  error(e) {
    this.#stream._error(e);
  }
}

class ReadableStream {
  constructor(source = {}, strategy = {}) {
    this._state = "readable";
    this._queue = [];
    this._reads = [];
    this._reader = null;
    this._storedError = undefined;
    this._closeRequested = false;
    this._pulling = false;
    this._pullAgain = false;
    this._started = false;
    this._disturbed = false;
    this._source = source;
    this._highWaterMark = strategy.highWaterMark === undefined ? 1 : Number(strategy.highWaterMark);
    this._closed = Promise.withResolvers();
    this._closed.promise.catch(() => {});
    const controller = (this._controller = new ReadableStreamDefaultController(this));
    const start = source.start;
    Promise.resolve(typeof start === "function" ? start.call(source, controller) : undefined).then(
      () => {
        this._started = true;
        this._pullIfNeeded();
      },
      (e) => this._error(e),
    );
  }

  get locked() {
    return this._reader !== null;
  }

  _pullIfNeeded() {
    if (!this._started || this._state !== "readable" || this._closeRequested) return;
    const pull = this._source.pull;
    if (typeof pull !== "function") return;
    if (this._reads.length === 0 && this._queue.length >= this._highWaterMark) return;
    if (this._pulling) {
      this._pullAgain = true;
      return;
    }
    this._pulling = true;
    Promise.resolve(pull.call(this._source, this._controller)).then(
      () => {
        this._pulling = false;
        if (this._pullAgain) {
          this._pullAgain = false;
          this._pullIfNeeded();
        }
      },
      (e) => this._error(e),
    );
  }

  _close() {
    if (this._state !== "readable") return;
    this._state = "closed";
    for (const r of this._reads.splice(0)) r.resolve({ value: undefined, done: true });
    this._closed.resolve();
  }

  _error(e) {
    if (this._state !== "readable") return;
    this._state = "errored";
    this._storedError = e;
    this._queue = [];
    for (const r of this._reads.splice(0)) r.reject(e);
    this._closed.reject(e);
  }

  _read() {
    this._disturbed = true;
    if (this._queue.length > 0) {
      const value = this._queue.shift();
      if (this._closeRequested && this._queue.length === 0) this._close();
      else this._pullIfNeeded();
      return Promise.resolve({ value, done: false });
    }
    if (this._state === "closed") return Promise.resolve({ value: undefined, done: true });
    if (this._state === "errored") return Promise.reject(this._storedError);
    const r = Promise.withResolvers();
    this._reads.push(r);
    this._pullIfNeeded();
    return r.promise;
  }

  _cancel(reason) {
    this._disturbed = true;
    if (this._state === "closed") return Promise.resolve();
    if (this._state === "errored") return Promise.reject(this._storedError);
    this._queue = [];
    this._close();
    const cancel = this._source.cancel;
    return Promise.resolve(typeof cancel === "function" ? cancel.call(this._source, reason) : undefined).then(() => {});
  }

  cancel(reason = undefined) {
    if (this.locked) return Promise.reject(new TypeError("Invalid state: ReadableStream is locked"));
    return this._cancel(reason);
  }

  getReader(options = undefined) {
    if (options !== undefined && options !== null && options.mode !== undefined) {
      throw new TypeError("js2wasi3: ReadableStream BYOB readers aren't supported");
    }
    if (this.locked) throw new TypeError("Invalid state: ReadableStream is locked");
    return new ReadableStreamDefaultReader(this);
  }

  async *values({ preventCancel = false } = {}) {
    const reader = this.getReader();
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) return;
        yield value;
      }
    } finally {
      if (!preventCancel && this._state === "readable") await reader.cancel();
      reader.releaseLock();
    }
  }

  [Symbol.asyncIterator](options) {
    return this.values(options);
  }

  get [Symbol.toStringTag]() {
    return "ReadableStream";
  }

  static from(iterable) {
    const it = typeof iterable[Symbol.asyncIterator] === "function"
      ? iterable[Symbol.asyncIterator]()
      : iterable[Symbol.iterator]();
    return new ReadableStream({
      async pull(c) {
        const { value, done } = await it.next();
        if (done) c.close();
        else c.enqueue(value);
      },
      async cancel(reason) {
        if (typeof it.return === "function") await it.return(reason);
      },
    }, { highWaterMark: 0 });
  }
}

class ReadableStreamDefaultReader {
  #stream;

  constructor(stream) {
    if (!(stream instanceof ReadableStream)) {
      throw new TypeError("ReadableStreamDefaultReader constructor: Argument 1 is not a ReadableStream.");
    }
    if (stream.locked) throw new TypeError("Invalid state: ReadableStream is locked");
    this.#stream = stream;
    stream._reader = this;
  }

  get closed() {
    return this.#stream === null ? Promise.reject(new TypeError("Invalid state: Reader released")) : this.#stream._closed.promise;
  }

  read() {
    if (this.#stream === null) return Promise.reject(new TypeError("Invalid state: Reader released"));
    return this.#stream._read();
  }

  cancel(reason = undefined) {
    if (this.#stream === null) return Promise.reject(new TypeError("Invalid state: Reader released"));
    return this.#stream._cancel(reason);
  }

  releaseLock() {
    if (this.#stream === null) return;
    const s = this.#stream;
    for (const r of s._reads.splice(0)) r.reject(new TypeError("Invalid state: Releasing reader"));
    s._reader = null;
    this.#stream = null;
  }

  get [Symbol.toStringTag]() {
    return "ReadableStreamDefaultReader";
  }
}

// ---- Blob ----

function toBytes(part) {
  if (typeof part === "string") return encoder.encode(part);
  if (part instanceof Blob) return part._bytes;
  if (part instanceof ArrayBuffer) return new Uint8Array(part.slice(0));
  if (ArrayBuffer.isView(part)) return new Uint8Array(part.buffer.slice(part.byteOffset, part.byteOffset + part.byteLength));
  return encoder.encode(String(part));
}

function concatBytes(chunks) {
  let n = 0;
  for (const c of chunks) n += c.length;
  const out = new Uint8Array(n);
  let at = 0;
  for (const c of chunks) {
    out.set(c, at);
    at += c.length;
  }
  return out;
}

class Blob {
  constructor(parts = [], options = {}) {
    if (typeof parts !== "object" || parts === null || typeof parts[Symbol.iterator] !== "function") {
      throw new TypeError("Blob constructor: Argument 1 could not be converted to: Sequence.");
    }
    this._bytes = concatBytes([...parts].map(toBytes));
    const type = options && options.type !== undefined ? String(options.type) : "";
    this._type = /^[\x20-\x7e]*$/.test(type) ? type.toLowerCase() : "";
  }

  get size() {
    return this._bytes.length;
  }

  get type() {
    return this._type;
  }

  slice(start = 0, end = this.size, type = "") {
    const n = this.size;
    const clamp = (x) => (x < 0 ? Math.max(n + x, 0) : Math.min(x, n));
    const b = new Blob([], { type });
    b._bytes = this._bytes.slice(clamp(start), clamp(end));
    return b;
  }

  text() {
    return Promise.resolve(new TextDecoder().decode(this._bytes));
  }

  arrayBuffer() {
    return Promise.resolve(this._bytes.slice().buffer);
  }

  bytes() {
    return Promise.resolve(this._bytes.slice());
  }

  stream() {
    const bytes = this._bytes;
    return new ReadableStream({
      start(c) {
        if (bytes.length > 0) c.enqueue(bytes.slice());
        c.close();
      },
    });
  }

  get [Symbol.toStringTag]() {
    return "Blob";
  }
}

// ---- Body ----

// The body and its default content type, from a BodyInit.
function extractBody(body) {
  if (typeof body === "string") return [encoder.encode(body), "text/plain;charset=UTF-8"];
  if (body instanceof URLSearchParams) {
    return [encoder.encode(body.toString()), "application/x-www-form-urlencoded;charset=UTF-8"];
  }
  if (body instanceof ArrayBuffer) return [new Uint8Array(body.slice(0)), null];
  if (ArrayBuffer.isView(body)) {
    return [new Uint8Array(body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength)), null];
  }
  if (body instanceof Blob) return [body._bytes, body.type || null];
  if (body instanceof ReadableStream) return [body, null];
  if (typeof body[Symbol.asyncIterator] === "function") return [ReadableStream.from(body), null];
  return [encoder.encode(String(body)), "text/plain;charset=UTF-8"];
}

async function readAll(stream) {
  const reader = stream.getReader();
  const chunks = [];
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    if (!(value instanceof Uint8Array)) throw new TypeError("Received non-Uint8Array chunk");
    chunks.push(value);
  }
  return concatBytes(chunks);
}

// A body is `null`, its bytes (`Uint8Array`) or a `ReadableStream`; `state.used` once read from bytes.
function bodyUsed(state) {
  return state.used || (state.body instanceof ReadableStream && state.body._disturbed);
}

// The bytes of the body (consumes it).
function consume(state) {
  if (bodyUsed(state)) return Promise.reject(new TypeError("Body is unusable: Body has already been read"));
  const b = state.body;
  if (b === null) return Promise.resolve(new Uint8Array(0));
  if (b instanceof Uint8Array) {
    state.used = true;
    return Promise.resolve(b);
  }
  return readAll(b);
}

function bodyStream(state) {
  const b = state.body;
  if (b === null || b instanceof ReadableStream) return b;
  // A stream over the bytes, made once.
  const s = new ReadableStream({
    start(c) {
      if (b.length > 0) c.enqueue(b);
      c.close();
    },
  });
  state.body = s;
  return s;
}

// A body stream's copy for `clone()`: reads it all, then gives each side the bytes.
function cloneBody(state) {
  const b = state.body;
  if (!(b instanceof ReadableStream)) return b;
  const all = readAll(b);
  const fresh = () => new ReadableStream({
    async start(c) {
      const bytes = await all;
      if (bytes.length > 0) c.enqueue(bytes.slice());
      c.close();
    },
  });
  state.body = fresh();
  return fresh();
}

// The Body mixin's members, in node's (undici's) order: `body`, `bodyUsed`, `between`'s, then the methods.
function defineBody(C, kind, between) {
  const descriptors = (o) => Object.getOwnPropertyDescriptors(o);
  const members = [
    descriptors({
      get body() {
        return bodyStream(this[kState]);
      },
      get bodyUsed() {
        return bodyUsed(this[kState]);
      },
    }),
    between,
    descriptors({
      clone() {
        if (bodyUsed(this[kState])) throw new TypeError(`${kind}.clone: Body has already been consumed.`);
        return cloneOf(this);
      },
      blob() {
        return consume(this[kState]).then((b) => {
          const blob = new Blob([], { type: this[kState].headers.get("content-type") || "" });
          blob._bytes = b.slice();
          return blob;
        });
      },
      arrayBuffer() {
        return consume(this[kState]).then((b) => b.slice().buffer);
      },
      text() {
        return consume(this[kState]).then((b) => new TextDecoder().decode(b));
      },
      json() {
        return this.text().then((t) => JSON.parse(t));
      },
      formData() {
        return Promise.reject(new TypeError("js2wasi3: formData() isn't supported"));
      },
      bytes() {
        return consume(this[kState]).then((b) => b.slice());
      },
    }),
  ];
  for (const group of members) {
    for (const key of Object.keys(group)) {
      Object.defineProperty(C.prototype, key, { ...group[key], enumerable: true, configurable: true });
    }
  }
}

function cloneOf(o) {
  const st = o[kState];
  if (o instanceof Response) {
    const r = new Response(null, { status: st.status === 0 ? 200 : st.status, statusText: st.statusText, headers: st.headers });
    Object.assign(r[kState], { type: st.type, url: st.url, status: st.status });
    r[kState].body = cloneBody(st);
    return r;
  }
  const r = new Request(o);
  st.used = false;
  r[kState].body = cloneBody(st);
  return r;
}

// ---- Response ----

const nullBodyStatus = (s) => s === 101 || s === 103 || s === 204 || s === 205 || s === 304;
const redirectStatus = (s) => s === 301 || s === 302 || s === 303 || s === 307 || s === 308;
const reasonRegExp = /^[\t\x20-\x7e\x80-\xff]*$/;

class Response {
  constructor(body = null, init = {}) {
    init = init === null || init === undefined ? {} : init;
    const status = init.status === undefined ? 200 : Number(init.status) | 0;
    if (status < 200 || status > 599) {
      throw new RangeError('init["status"] must be in the range of 200 to 599, inclusive.');
    }
    const statusText = init.statusText === undefined ? "" : String(init.statusText);
    if (!reasonRegExp.test(statusText)) throw new TypeError("Invalid statusText");
    const headers = new Headers();
    headers[kGuard] = "response";
    if (init.headers !== undefined) fillHeaders(headers, init.headers, "Response constructor");
    const st = { type: "default", url: "", redirected: false, status, statusText, headers, body: null, used: false };
    Object.defineProperty(this, kState, { value: st });
    if (body !== null && body !== undefined) {
      if (nullBodyStatus(status)) throw new TypeError(`Response constructor: Invalid response status code ${status}`);
      const [b, type] = extractBody(body);
      st.body = b;
      if (type !== null && !headers.has("content-type")) headers.append("content-type", type);
    }
  }

  get type() {
    return this[kState].type;
  }

  get url() {
    return this[kState].url;
  }

  get redirected() {
    return this[kState].redirected;
  }

  get status() {
    return this[kState].status;
  }

  get ok() {
    const s = this[kState].status;
    return s >= 200 && s <= 299;
  }

  get statusText() {
    return this[kState].statusText;
  }

  get headers() {
    return this[kState].headers;
  }

  get [Symbol.toStringTag]() {
    return "Response";
  }

  static error() {
    const r = new Response(null, { status: 200 });
    r[kState].type = "error";
    r[kState].status = 0;
    return r;
  }

  static json(data, init = {}) {
    const body = JSON.stringify(data);
    if (body === undefined) throw new TypeError("Value is not JSON serializable");
    const r = new Response(body, init);
    const given = init && init.headers !== undefined ? new Headers(init.headers).get("content-type") : null;
    r[kState].headers.set("content-type", given === null ? "application/json" : given);
    return r;
  }

  static redirect(url, status = 302) {
    let parsed;
    try {
      parsed = new URL(String(url));
    } catch (e) {
      throw new TypeError(`Failed to parse URL from ${url}`, { cause: e });
    }
    if (!redirectStatus(status)) throw new RangeError(`Invalid status code ${status}`);
    const r = new Response(null, { status });
    r[kState].headers.set("location", parsed.href);
    return r;
  }
}
defineBody(Response, "Response", {});

// ---- Request ----

const normalMethods = ["DELETE", "GET", "HEAD", "OPTIONS", "POST", "PUT"];

class Request {
  constructor(input, init = {}) {
    if (arguments.length === 0) {
      throw new TypeError("Request constructor: 1 argument required, but only 0 present.");
    }
    init = init === null || init === undefined ? {} : init;
    const from = input instanceof Request ? input[kState] : null;
    let url;
    if (from !== null) {
      url = from.url;
    } else {
      try {
        url = new URL(String(input)).href;
      } catch (e) {
        throw new TypeError(`Failed to parse URL from ${input}`, { cause: e });
      }
    }
    let method = init.method !== undefined ? String(init.method) : from ? from.method : "GET";
    if (init.method !== undefined) {
      if (!isToken(method)) throw new TypeError(`'${method}' is not a valid HTTP method.`);
      const upper = method.toUpperCase();
      if (upper === "CONNECT" || upper === "TRACE" || upper === "TRACK") {
        throw new TypeError(`'${method}' HTTP method is unsupported.`);
      }
      if (normalMethods.includes(upper)) method = upper;
    }
    const headers = new Headers();
    headers[kGuard] = "request";
    const headersInit = init.headers !== undefined ? init.headers : from ? from.headers : undefined;
    if (headersInit !== undefined) fillHeaders(headers, headersInit, "Request constructor");
    const pick = (key, dflt) => (init[key] !== undefined ? String(init[key]) : from ? from[key] : dflt);
    const st = {
      url,
      method,
      headers,
      redirect: pick("redirect", "follow"),
      mode: pick("mode", "cors"),
      credentials: pick("credentials", "same-origin"),
      cache: pick("cache", "default"),
      signal: init.signal !== undefined && init.signal !== null ? init.signal : from ? from.signal : undefined,
      duplex: init.duplex !== undefined ? String(init.duplex) : "half",
      body: null,
      used: false,
    };
    Object.defineProperty(this, kState, { value: st });
    const body = init.body !== undefined ? init.body : from ? from.body : null;
    if (body !== null && body !== undefined) {
      if (method === "GET" || method === "HEAD") throw new TypeError("Request with GET/HEAD method cannot have body.");
      if (from !== null && init.body === undefined) {
        st.body = body;
        from.used = true;
      } else {
        const [b, type] = extractBody(body);
        if (b instanceof ReadableStream && init.duplex === undefined) {
          throw new TypeError("RequestInit: duplex option is required when sending a body.");
        }
        st.body = b;
        if (type !== null && !headers.has("content-type")) headers.append("content-type", type);
      }
    }
  }

  get method() {
    return this[kState].method;
  }

  get url() {
    return this[kState].url;
  }

  get headers() {
    return this[kState].headers;
  }

  get destination() {
    return "";
  }

  get referrer() {
    return "about:client";
  }

  get referrerPolicy() {
    return "";
  }

  get mode() {
    return this[kState].mode;
  }

  get credentials() {
    return this[kState].credentials;
  }

  get cache() {
    return this[kState].cache;
  }

  get redirect() {
    return this[kState].redirect;
  }

  get integrity() {
    return "";
  }

  get keepalive() {
    return false;
  }

  get isReloadNavigation() {
    return false;
  }

  get isHistoryNavigation() {
    return false;
  }

  get signal() {
    const st = this[kState];
    if (st.signal === undefined) {
      const { AbortController } = require("node:internal/abort");
      st.signal = new AbortController().signal;
    }
    return st.signal;
  }

  get [Symbol.toStringTag]() {
    return "Request";
  }
}
defineBody(Request, "Request", Object.getOwnPropertyDescriptors({
  get duplex() {
    return this[kState].duplex;
  },
}));
// (undici has this own property too.)
Object.defineProperty(Request.prototype, "attribute", { value: undefined, enumerable: true });

for (const [name, C] of [["Headers", Headers], ["Request", Request], ["Response", Response],
  ["ReadableStream", ReadableStream], ["ReadableStreamDefaultReader", ReadableStreamDefaultReader],
  ["ReadableStreamDefaultController", ReadableStreamDefaultController], ["Blob", Blob]]) {
  if (globalThis[name] === undefined) {
    Object.defineProperty(globalThis, name, { value: C, writable: true, enumerable: false, configurable: true });
  }
}

module.exports = {
  Headers,
  Request,
  Response,
  ReadableStream,
  ReadableStreamDefaultReader,
  ReadableStreamDefaultController,
  Blob,
  // For `fetch` (T72, `node:internal/fetch_client`).
  kState,
  kAppend,
};
