// node's `http2` (T65): loads with node 24's constants and settings; sessions and servers throw (planned: F286).
"use strict";

const { Readable } = require("stream");
const Stream = require("stream");
const data = require("node:internal/builtin_data");
const { unavailableError, fn } = require("node:internal/unavailable");

const WHY = "no HTTP/2 over WASI sockets yet";

class Http2ServerRequest extends Readable {
  constructor() {
    super();
    throw unavailableError("http2.Http2ServerRequest", WHY);
  }
}

class Http2ServerResponse extends Stream {
  constructor() {
    super();
    throw unavailableError("http2.Http2ServerResponse", WHY);
  }
}

module.exports = {
  connect: fn("http2", "connect", WHY),
  constants: { ...data.http2Constants },
  createServer: fn("http2", "createServer", WHY),
  createSecureServer: fn("http2", "createSecureServer", WHY),
  getDefaultSettings: () => ({ __proto__: null, ...data.http2DefaultSettings }),
  getPackedSettings: fn("http2", "getPackedSettings", WHY),
  getUnpackedSettings: fn("http2", "getUnpackedSettings", WHY),
  performServerHandshake: fn("http2", "performServerHandshake", WHY),
  sensitiveHeaders: Symbol("sensitiveHeaders"),
  Http2ServerRequest,
  Http2ServerResponse,
};
