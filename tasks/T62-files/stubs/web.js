"use strict";
// T62 survey: stand-ins for globals js2wasi3 lacks (web platform, Intl, DataView, Proxy).
function cls(n) { return class { constructor() { throw new Error("T62 stub: new " + n); } static canParse() { throw new Error("T62 stub: " + n + ".canParse"); } static createObjectURL() { throw new Error("T62 stub: " + n + ".createObjectURL"); } }; }
exports.Response = cls("Response");
exports.Request = cls("Request");
exports.ReadableStream = cls("ReadableStream");
exports.DataView = cls("DataView");
exports.Blob = cls("Blob");
exports.TextDecoder = cls("TextDecoder");
exports.TextEncoder = cls("TextEncoder");
exports.Headers = cls("Headers");
exports.WebSocket = cls("WebSocket");
exports.Proxy = cls("Proxy");
exports.MessageChannel = cls("MessageChannel");
exports.navigator = undefined;
exports.fetch = function fetch() { throw new Error("T62 stub: fetch"); };
exports.crypto = { randomUUID() { throw new Error("T62 stub: crypto.randomUUID"); }, getRandomValues() { throw new Error("T62 stub: crypto.getRandomValues"); } };
exports.Intl = { DateTimeFormat: function DateTimeFormat() { return { resolvedOptions() { return { timeZone: "UTC" }; } }; } };
// Working stand-ins (T62 run-time probe): over Buffer.
exports.TextEncoder = class TextEncoder { get encoding() { return "utf-8"; } encode(s = "") { return new Uint8Array(Buffer.from(String(s), "utf8")); } };
exports.TextDecoder = class TextDecoder { constructor(e = "utf-8") { this.encoding = e; } decode(b) { return b === undefined ? "" : Buffer.from(b.buffer ?? b, b.byteOffset ?? 0, b.byteLength).toString("utf8"); } };
// Minimal URL (T62 run-time probe): scheme://[user@]host[:port][/path][?query][#hash] only.
exports.URL = class URL {
  constructor(u, base) {
    const m = /^([a-z][a-z0-9+.-]*:)\/\/(?:[^@/]*@)?(\[[^\]]*\]|[^:/?#]*)(?::(\d*))?([^?#]*)(\?[^#]*)?(#.*)?$/i.exec(String(u));
    if (!m) throw Object.assign(new TypeError("Invalid URL"), { code: "ERR_INVALID_URL", input: String(u) });
    this.protocol = m[1].toLowerCase(); this.hostname = m[2]; this.port = m[3] ?? ""; this.pathname = m[4] || "/";
    this.search = m[5] ?? ""; this.hash = m[6] ?? ""; this.host = this.hostname + (this.port ? ":" + this.port : "");
    this.origin = this.protocol + "//" + this.host; this.href = this.origin + this.pathname + this.search + this.hash;
    this.username = ""; this.password = "";
  }
  toString() { return this.href; }
  static canParse(u) { try { new URL(u); return true; } catch { return false; } }
};
