// node's `http` (T65): the module loads with node 24's shape so packages that name it at load work
// (`class extends IncomingMessage`, `http.STATUS_CODES`, `new http.Agent()`, header validation).
// Servers and requests need sockets: `createServer`, `request`, `get` and the `Server`/`ClientRequest`
// constructors throw (planned: F286, over T66's `net`).
"use strict";

const EventEmitter = require("events");
const Stream = require("stream");
const { Readable } = Stream;
const data = require("node:internal/builtin_data");
const { unavailableError, fn, fns } = require("node:internal/unavailable");

const WHY = "no HTTP over WASI sockets yet";
const tokenRegExp = /^[\^_`a-zA-Z\-0-9!#$%&'*+.|~]+$/;
const headerCharRegex = /[^\t\x20-\x7e\x80-\xff]/;

function codeError(Ctor, code, message) {
  const e = new Ctor(message);
  e.code = code;
  return e;
}

function validateHeaderName(name, label = "Header name") {
  if (typeof name !== "string" || !name || !tokenRegExp.test(name)) {
    throw codeError(TypeError, "ERR_INVALID_HTTP_TOKEN", `${label} must be a valid HTTP token ["${name}"]`);
  }
}

function validateHeaderValue(name, value) {
  if (value === undefined) {
    throw codeError(TypeError, "ERR_HTTP_INVALID_HEADER_VALUE", `Invalid value "${value}" for header "${name}"`);
  }
  if (headerCharRegex.test(String(value))) {
    throw codeError(TypeError, "ERR_INVALID_CHAR", `Invalid character in header content ["${name}"]`);
  }
}

class Agent extends EventEmitter {
  constructor(options) {
    super();
    this.options = { __proto__: null, ...options };
    if (this.options.noDelay === undefined) this.options.noDelay = true;
    this.options.path = null;
    this.defaultPort = 80;
    this.protocol = "http:";
    this.requests = { __proto__: null };
    this.sockets = { __proto__: null };
    this.freeSockets = { __proto__: null };
    this.keepAliveMsecs = this.options.keepAliveMsecs || 1000;
    this.keepAlive = this.options.keepAlive || false;
    this.maxSockets = this.options.maxSockets || Agent.defaultMaxSockets;
    this.maxFreeSockets = this.options.maxFreeSockets || 256;
    this.scheduling = this.options.scheduling || "lifo";
    this.maxTotalSockets = this.options.maxTotalSockets ?? Infinity;
    this.totalSocketCount = 0;
    this.agentKeepAliveTimeoutBuffer = this.options.agentKeepAliveTimeoutBuffer ?? 1000;
  }

  getName(options = {}) {
    let name = options.host || "localhost";
    name += ":";
    if (options.port) name += options.port;
    name += ":";
    if (options.localAddress) name += options.localAddress;
    if (options.family === 4 || options.family === 6) name += `:${options.family}`;
    if (options.socketPath) name += `:${options.socketPath}`;
    return name;
  }

  destroy() {}
}
Agent.defaultMaxSockets = Infinity;
fns(Agent.prototype, "Agent", ["createConnection", "addRequest", "createSocket", "removeSocket", "keepSocketAlive", "reuseSocket"], WHY);

class IncomingMessage extends Readable {
  constructor(socket) {
    super({ autoDestroy: false });
    this._readableState.readingMore = true;
    this.socket = socket;
    this.httpVersionMajor = null;
    this.httpVersionMinor = null;
    this.httpVersion = null;
    this.complete = false;
    this.rawHeaders = [];
    this.rawTrailers = [];
    this.joinDuplicateHeaders = false;
    this.aborted = false;
    this.upgrade = null;
    this.url = "";
    this.method = null;
    this.statusCode = null;
    this.statusMessage = null;
    this.client = socket;
    this._consuming = false;
    this._dumped = false;
  }

  // Filled by node's parser, which js2wasi3 doesn't have: `rawHeaders` doesn't feed it.
  get headers() {
    return this._headers === undefined ? (this._headers = {}) : this._headers;
  }

  set headers(value) {
    this._headers = value;
  }

  get trailers() {
    return this._trailers === undefined ? (this._trailers = {}) : this._trailers;
  }

  set trailers(value) {
    this._trailers = value;
  }

  setTimeout(msecs, callback) {
    if (callback) this.on("timeout", callback);
    return this;
  }

  _read() {}
}

const kHeaders = Symbol("kOutHeaders");

class OutgoingMessage extends Stream {
  constructor() {
    super();
    this.outputData = [];
    this.outputSize = 0;
    this.writable = true;
    this.destroyed = false;
    this._last = false;
    this.chunkedEncoding = false;
    this.shouldKeepAlive = true;
    this.maxRequestsOnConnectionReached = false;
    this._defaultKeepAlive = true;
    this.useChunkedEncodingByDefault = true;
    this.sendDate = false;
    this._removedConnection = false;
    this._removedContLen = false;
    this._removedTE = false;
    this.strictContentLength = false;
    this._contentLength = null;
    this._hasBody = true;
    this._trailer = "";
    this.finished = false;
    this._headerSent = false;
    this[kHeaders] = null;
    this.socket = null;
    this._header = null;
  }

  get headersSent() {
    return this._header !== null;
  }

  setHeader(name, value) {
    validateHeaderName(name);
    validateHeaderValue(name, value);
    if (this[kHeaders] === null) this[kHeaders] = { __proto__: null };
    this[kHeaders][name.toLowerCase()] = [name, value];
    return this;
  }

  appendHeader(name, value) {
    validateHeaderName(name);
    validateHeaderValue(name, value);
    const key = name.toLowerCase();
    if (this[kHeaders] === null) this[kHeaders] = { __proto__: null };
    const old = this[kHeaders][key];
    if (old === undefined) this[kHeaders][key] = [name, value];
    else this[kHeaders][key] = [old[0], [].concat(old[1], value)];
    return this;
  }

  getHeader(name) {
    if (typeof name !== "string") {
      throw $$errInvalidArgType("name", "of type string", name);
    }
    const entry = this[kHeaders] === null ? undefined : this[kHeaders][name.toLowerCase()];
    return entry && entry[1];
  }

  getHeaderNames() {
    return this[kHeaders] === null ? [] : Object.keys(this[kHeaders]);
  }

  getRawHeaderNames() {
    return this[kHeaders] === null ? [] : Object.values(this[kHeaders]).map((e) => e[0]);
  }

  getHeaders() {
    const out = { __proto__: null };
    if (this[kHeaders] !== null) for (const key of Object.keys(this[kHeaders])) out[key] = this[kHeaders][key][1];
    return out;
  }

  hasHeader(name) {
    if (typeof name !== "string") {
      throw $$errInvalidArgType("name", "of type string", name);
    }
    return this[kHeaders] !== null && !!this[kHeaders][name.toLowerCase()];
  }

  removeHeader(name) {
    if (typeof name !== "string") {
      throw $$errInvalidArgType("name", "of type string", name);
    }
    if (this[kHeaders] !== null) delete this[kHeaders][name.toLowerCase()];
  }

  setTimeout(msecs, callback) {
    if (callback) this.on("timeout", callback);
    return this;
  }
}
fns(OutgoingMessage.prototype, "OutgoingMessage", ["write", "end", "flushHeaders", "addTrailers", "cork", "uncork", "destroy"], WHY);

class ServerResponse extends OutgoingMessage {
  constructor(req, options) {
    super(options);
    if (req.method === "HEAD") this._hasBody = false;
    this.req = req;
    this.sendDate = true;
    this._sent100 = false;
    this._expect_continue = false;
    this.statusCode = 200;
    this.statusMessage = undefined;
  }
}
fns(ServerResponse.prototype, "ServerResponse", ["writeHead", "writeContinue", "writeProcessing", "writeEarlyHints", "assignSocket", "detachSocket"], WHY);

class ClientRequest extends OutgoingMessage {
  constructor() {
    super();
    throw unavailableError("http.ClientRequest", WHY);
  }
}

class Server extends EventEmitter {
  constructor() {
    super();
    throw unavailableError("http.Server", WHY);
  }
}

// node's `WebSocket`, `CloseEvent`, `MessageEvent` come from undici.
function webClass(name) {
  const C = class {
    constructor() {
      throw unavailableError(name, WHY);
    }
  };
  Object.defineProperty(C, "name", { value: name });
  return C;
}
const WebSocket = webClass("WebSocket");
const CloseEvent = webClass("CloseEvent");
const MessageEvent = webClass("MessageEvent");

let globalAgent;

function setMaxIdleHTTPParsers(max) {
  if (typeof max !== "number" || !Number.isInteger(max)) {
    throw $$errInvalidArgType("max", "of type number", max);
  }
  if (max < 1) throw $$errOutOfRange("max", ">= 1", max);
}

module.exports = {
  _connectionListener: fn("http", "_connectionListener", WHY),
  METHODS: data.httpMethods.slice(),
  STATUS_CODES: { ...data.statusCodes },
  Agent,
  ClientRequest,
  IncomingMessage,
  OutgoingMessage,
  Server,
  ServerResponse,
  createServer: fn("http", "createServer", WHY),
  validateHeaderName,
  validateHeaderValue,
  get: fn("http", "get", WHY),
  request: fn("http", "request", WHY),
  setMaxIdleHTTPParsers,
  setGlobalProxyFromEnv: fn("http", "setGlobalProxyFromEnv", WHY),
};
Object.defineProperties(module.exports, {
  maxHeaderSize: { configurable: true, enumerable: true, get: () => data.maxHeaderSize },
  globalAgent: {
    configurable: true,
    enumerable: true,
    get: () => globalAgent || (globalAgent = new Agent({ keepAlive: true, scheduling: "lifo", timeout: 5000 })),
    set: (value) => { globalAgent = value; },
  },
  WebSocket: { configurable: true, enumerable: true, get: () => WebSocket },
  CloseEvent: { configurable: true, enumerable: true, get: () => CloseEvent },
  MessageEvent: { configurable: true, enumerable: true, get: () => MessageEvent },
});
