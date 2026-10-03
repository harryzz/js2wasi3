// node's `http`: the server (T71) is node 24's own `_http_server`, `_http_incoming`, `_http_outgoing`,
// `_http_common` (`scripts/gen-node-http.mjs`) over T66's `net` and a JS llhttp (`node:internal/http_parser`).
// The client (`request`, `get`, `ClientRequest`, `Agent`'s sockets) isn't there: it throws (T65's shape).
"use strict";

const EventEmitter = require("events");
const data = require("node:internal/builtin_data");
const { unavailableError, fn, fns } = require("node:internal/unavailable");
const { validateInteger } = require("node:internal/validators");
const { parsers } = require("node:_http_common");
const { IncomingMessage } = require("node:_http_incoming");
const { validateHeaderName, validateHeaderValue, OutgoingMessage } = require("node:_http_outgoing");
const { _connectionListener, STATUS_CODES, Server, ServerResponse } = require("node:_http_server");

const WHY = "no HTTP client over WASI sockets yet";

function createServer(opts, requestListener) {
  return new Server(opts, requestListener);
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

class ClientRequest extends OutgoingMessage {
  constructor() {
    super();
    throw unavailableError("http.ClientRequest", WHY);
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
  validateInteger(max, "max", 1);
  parsers.max = max;
}

module.exports = {
  _connectionListener,
  METHODS: data.httpMethods.slice(),
  STATUS_CODES,
  Agent,
  ClientRequest,
  IncomingMessage,
  OutgoingMessage,
  Server,
  ServerResponse,
  createServer,
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
