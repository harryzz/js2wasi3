// node's `https` (T65): loads with node 24's shape; servers and requests throw (planned: F286).
"use strict";

const http = require("http");
const tls = require("tls");
const { fn } = require("node:internal/unavailable");

const WHY = "no HTTP over WASI sockets yet";

class Agent extends http.Agent {
  constructor(options) {
    super(options);
    this.defaultPort = 443;
    this.protocol = "https:";
    this.maxCachedSessions = this.options.maxCachedSessions;
    if (this.maxCachedSessions === undefined) this.maxCachedSessions = 100;
    this._sessionCache = { map: {}, list: [] };
  }
}

class Server extends tls.Server {}

module.exports = {
  Agent,
  globalAgent: new Agent({ keepAlive: true, scheduling: "lifo", timeout: 5000 }),
  Server,
  createServer: fn("https", "createServer", WHY),
  get: fn("https", "get", WHY),
  request: fn("https", "request", WHY),
};
