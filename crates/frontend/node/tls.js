// node's `tls` (T65): loads with node 24's shape and defaults; connections and contexts throw (planned: F281).
// `rootCertificates` is empty: js2wasi3 doesn't carry node's CA store (F281).
"use strict";

const EventEmitter = require("events");
const { Duplex } = require("stream");
const { Buffer } = require("buffer");
const data = require("node:internal/builtin_data");
const { unavailableError, fn } = require("node:internal/unavailable");

const WHY = "no TLS over WASI sockets yet";

class SecureContext {
  constructor() {
    throw unavailableError("tls.SecureContext", WHY);
  }
}

class TLSSocket extends Duplex {
  constructor() {
    super();
    throw unavailableError("tls.TLSSocket", WHY);
  }
}

class Server extends EventEmitter {
  constructor() {
    super();
    throw unavailableError("tls.Server", WHY);
  }
}

// node's wire format: each protocol name prefixed by its length.
function convertALPNProtocols(protocols, out) {
  if (Array.isArray(protocols)) {
    const parts = [];
    for (const p of protocols) {
      const bytes = Buffer.from(p);
      if (bytes.length > 255) {
        throw $$errOutOfRange("The byte length of the protocol at index 0", "<= 255", bytes.length);
      }
      parts.push(Buffer.from([bytes.length]), bytes);
    }
    out.ALPNProtocols = Buffer.concat(parts);
  } else if (ArrayBuffer.isView(protocols)) {
    out.ALPNProtocols = Buffer.from(protocols.buffer.slice(protocols.byteOffset, protocols.byteOffset + protocols.byteLength));
  }
}

const rootCertificates = Object.freeze([]);

module.exports = {
  CLIENT_RENEG_LIMIT: 3,
  CLIENT_RENEG_WINDOW: 600,
  DEFAULT_CIPHERS: data.tlsDefaultCiphers,
  DEFAULT_ECDH_CURVE: "auto",
  DEFAULT_MIN_VERSION: "TLSv1.2",
  DEFAULT_MAX_VERSION: "TLSv1.3",
  getCiphers: () => data.tlsCiphers.slice(),
  getCertificateCompressionAlgorithms: fn("tls", "getCertificateCompressionAlgorithms", WHY),
  getCACertificates: fn("tls", "getCACertificates", WHY),
  setDefaultCACertificates: fn("tls", "setDefaultCACertificates", WHY),
  convertALPNProtocols,
  checkServerIdentity: fn("tls", "checkServerIdentity", WHY),
  createSecureContext: fn("tls", "createSecureContext", WHY),
  SecureContext,
  TLSSocket,
  Server,
  createServer: fn("tls", "createServer", WHY),
  connect: fn("tls", "connect", WHY),
};
Object.defineProperty(module.exports, "rootCertificates", {
  configurable: false,
  enumerable: true,
  get: () => rootCertificates,
});
