// node's `dgram` (T65): loads with node 24's shape; sockets throw (planned: F282, over `wasi:sockets` UDP).
"use strict";

const EventEmitter = require("events");
const { unavailableError, fn } = require("node:internal/unavailable");

const WHY = "no UDP over WASI sockets yet";

class Socket extends EventEmitter {
  constructor() {
    super();
    throw unavailableError("dgram.Socket", WHY);
  }
}

module.exports = {
  _createSocketHandle: fn("dgram", "_createSocketHandle", WHY),
  createSocket: fn("dgram", "createSocket", WHY),
  Socket,
};
