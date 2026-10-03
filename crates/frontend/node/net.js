// node's `net` (T66): TCP over `wasi:sockets` on the event loop. `Socket` is a `stream.Duplex`
// (node 24's stream code, T60): `connect` looks the host up (`ip-name-lookup`), connects, then
// pumps `_write`'s chunks into the socket's `send` stream and pushes what its `receive` stream
// gives; `Server` binds, listens and makes a `Socket` per accepted connection. IPC paths, `BlockList`,
// `SocketAddress` and handle passing aren't there.
"use strict";

const EventEmitter = require("events");
const { Duplex } = require("stream");
const { Buffer } = require("buffer");
const wasi = require("node:internal/wasi_sockets");
const support = require("node:internal/net");
const { unavailableError, fn } = require("node:internal/unavailable");

const { isIP, isIPv4, isIPv6, socketAddress, addressInfo, errnoError, lookupAll } = support;

function errMissingArgs() {
  const e = new TypeError('The "options" or "port" or "path" argument must be specified');
  e.code = "ERR_MISSING_ARGS";
  return e;
}

// node's `validatePort` (lib/internal/validators.js).
function validatePort(port, name = "Port", allowZero = true) {
  if ((typeof port !== "number" && typeof port !== "string") ||
      (typeof port === "string" && port.trim().length === 0) ||
      +port !== (+port >>> 0) ||
      port > 0xFFFF ||
      (port === 0 && !allowZero)) {
    const e = new RangeError(`${name} should be ${allowZero ? ">=" : ">"} 0 and < 65536. Received ${$$nodeSpecificType(port)}.`);
    e.code = "ERR_SOCKET_BAD_PORT";
    throw e;
  }
  return port | 0;
}

function toNumber(x) {
  return (x = Number(x)) >= 0 ? x : false;
}

function isPipeName(s) {
  return typeof s === "string" && toNumber(s) === false;
}

const normalizedArgs = Symbol("normalizedArgs");

// node's `normalizeArgs`: `[options, cb]` from `(options[, cb])`, `(path[, cb])` or
// `([port][, host][, …][, cb])`.
function normalizeArgs(args) {
  let arr;
  if (args.length === 0) {
    arr = [{}, null];
    arr[normalizedArgs] = true;
    return arr;
  }
  const arg0 = args[0];
  let options = {};
  if (typeof arg0 === "object" && arg0 !== null) {
    options = arg0;
  } else if (isPipeName(arg0)) {
    options.path = arg0;
  } else {
    options.port = arg0;
    if (args.length > 1 && typeof args[1] === "string") options.host = args[1];
  }
  const cb = args[args.length - 1];
  arr = typeof cb !== "function" ? [options, null] : [options, cb];
  arr[normalizedArgs] = true;
  return arr;
}

// The chunks `_write` queues, as the async iterable `send` reads (one socket's).
async function* sendChunks(socket) {
  const q = socket._sendQueue;
  for (;;) {
    while (q.length) {
      const [chunk, cb] = q.shift();
      yield chunk;
      socket._bytesDispatched += chunk.length;
      cb();
    }
    if (socket._sendEnded) return;
    await new Promise((resolve) => {
      socket._sendWake = resolve;
    });
  }
}

function wake(socket) {
  const w = socket._sendWake;
  if (w) {
    socket._sendWake = null;
    w();
  }
}

class Socket extends Duplex {
  constructor(options) {
    if (typeof options === "number") options = { fd: options };
    else options = { ...options };
    if (options.fd !== undefined) throw unavailableError("net.Socket({ fd })", "no file descriptors over WASI");
    options.allowHalfOpen = Boolean(options.allowHalfOpen);
    options.emitClose = false;
    options.autoDestroy = true;
    options.decodeStrings = false;
    super(options);
    this.connecting = false;
    this._hadError = false;
    this._handle = null;
    this._opened = false;
    this._parent = null;
    this._host = null;
    this._peername = null;
    this._sockname = null;
    this._closeAfterHandlingError = false;
    this._sendQueue = [];
    this._sendEnded = false;
    this._sendWake = null;
    this._sent = null;
    this._finalCb = null;
    this._reader = null;
    this._readWake = null;
    this._timer = null;
    this._keepAlive = false;
    this._bytesDispatched = 0;
    this.bytesRead = 0;
    this.server = null;
    this._server = null;
    this.timeout = undefined;
    if (options.handle !== undefined) {
      this._handle = options.handle;
      this._opened = true;
      this._connected();
    }
  }

  get pending() {
    return !this._handle || this.connecting;
  }

  get readyState() {
    if (this.connecting) return "opening";
    if (this.readable && this.writable) return "open";
    if (this.readable && !this.writable) return "readOnly";
    if (!this.readable && this.writable) return "writeOnly";
    return "closed";
  }

  get bytesWritten() {
    let n = this._bytesDispatched;
    for (const [chunk] of this._sendQueue) n += chunk.length;
    return n;
  }

  get remoteAddress() {
    return this._peername ? this._peername.address : undefined;
  }

  get remoteFamily() {
    return this._peername ? this._peername.family : undefined;
  }

  get remotePort() {
    return this._peername ? this._peername.port : undefined;
  }

  get localAddress() {
    return this._sockname ? this._sockname.address : undefined;
  }

  get localPort() {
    return this._sockname ? this._sockname.port : undefined;
  }

  get localFamily() {
    return this._sockname ? this._sockname.family : undefined;
  }

  address() {
    return this._sockname ? { ...this._sockname } : {};
  }

  connect(...args) {
    const normalized = Array.isArray(args[0]) && args[0][normalizedArgs] ? args[0] : normalizeArgs(args);
    const options = normalized[0];
    const cb = normalized[1];
    if (cb !== null) this.once("connect", cb);
    if (options.port === undefined && options.path == null) throw errMissingArgs();
    if (options.path != null) throw unavailableError("net.Socket.connect({ path })", "no IPC sockets over WASI");
    if (this.destroyed) {
      this._undestroy();
      this._handle = null;
      this._peername = null;
      this._sockname = null;
    }
    const host = options.host || "localhost";
    if (typeof host !== "string") throw $$errInvalidArgType("options.host", "of type string", host);
    let port = options.port;
    if (port !== undefined) {
      if (typeof port !== "number" && typeof port !== "string") {
        throw $$errInvalidArgType("options.port", "one of type number or string", port);
      }
      validatePort(port);
    }
    port |= 0;
    this.connecting = true;
    this._opened = true;
    this._host = host;
    const family = isIP(host);
    if (family) {
      process.nextTick(() => {
        if (this.connecting) this._attempt([{ address: host, family }], port);
      });
    } else {
      lookupAll(host, options.family | 0, "verbatim").then(
        (addresses) => {
          if (!this.connecting) return;
          this.emit("lookup", null, addresses[0].address, addresses[0].family, host);
          this._attempt(addresses, port);
        },
        (err) => {
          if (!this.connecting) return;
          this.emit("lookup", err, undefined, undefined, host);
          this.destroy(err);
        },
      );
    }
    return this;
  }

  // Connect to each address in turn until one connects.
  async _attempt(addresses, port) {
    const errors = [];
    for (const { address, family } of addresses) {
      this.emit("connectionAttempt", address, port, family);
      let h;
      try {
        h = wasi.create(family === 6 ? "ipv6" : "ipv4");
        await wasi.connect(h, socketAddress(address, port));
      } catch (e) {
        if (h !== undefined) wasi.drop(h);
        if (!this.connecting) return;
        const err = errnoError(e, "connect", address, port);
        this.emit("connectionAttemptFailed", address, port, family, err);
        errors.push(err);
        continue;
      }
      if (!this.connecting) {
        wasi.drop(h);
        return;
      }
      this._handle = h;
      this._connected();
      this.emit("connect");
      this.emit("ready");
      return;
    }
    let err = errors[0];
    if (errors.length > 1) {
      err = new AggregateError(errors);
      err.code = errors[0].code;
    }
    this.destroy(err);
  }

  // A connected handle: start sending and receiving.
  _connected() {
    const h = this._handle;
    this.connecting = false;
    try {
      this._peername = addressInfo(wasi.getRemoteAddress(h));
      this._sockname = addressInfo(wasi.getLocalAddress(h));
    } catch {
      // (Left unknown.)
    }
    if (this._keepAlive) this._applyKeepAlive();
    this._sent = wasi.send(h, sendChunks(this)).then((r) => {
      if (r && r.tag === "err" && !this.destroyed) this.destroy(errnoError(r.val, "write"));
      const cb = this._finalCb;
      this._finalCb = null;
      if (cb) cb();
    });
    const [rx, done] = wasi.receive(h);
    this._reader = rx;
    this._receive(rx, done);
  }

  async _receive(rx, done) {
    try {
      for await (const chunk of rx) {
        if (this.destroyed) break;
        this.bytesRead += chunk.length;
        this._active();
        if (!this.push(Buffer.from(chunk.buffer, chunk.byteOffset, chunk.length))) {
          await new Promise((resolve) => {
            this._readWake = resolve;
          });
        }
        if (this.destroyed) break;
      }
    } catch (e) {
      if (!this.destroyed) this.destroy(errnoError(e, "read"));
      return;
    }
    if (this.destroyed) return;
    const r = await done;
    if (this.destroyed) return;
    if (r && r.tag === "err") {
      this.destroy(errnoError(r.val, "read"));
      return;
    }
    // (As node's `onStreamRead`: a paused socket still ends.)
    this.push(null);
    this.read(0);
  }

  _read() {
    const w = this._readWake;
    if (w) {
      this._readWake = null;
      w();
    }
  }

  _write(chunk, encoding, cb) {
    if (typeof chunk === "string") chunk = Buffer.from(chunk, encoding);
    this._active();
    this._sendQueue.push([chunk, cb]);
    wake(this);
  }

  _final(cb) {
    // Once connected, end the `send` stream; done when the host has sent it all.
    if (this.pending) {
      this.once("connect", () => this._final(cb));
      return;
    }
    this._finalCb = cb;
    this._sendEnded = true;
    wake(this);
  }

  _destroy(err, cb) {
    this.connecting = false;
    this._clearTimer();
    const h = this._handle;
    if (h !== null) {
      this._handle = null;
      this._sendEnded = true;
      wake(this);
      for (const [, writeCb] of this._sendQueue.splice(0)) writeCb(err || null);
      const rx = this._reader;
      this._reader = null;
      if (rx !== null) {
        rx.cancel();
        rx.return();
      }
      this._read();
      wasi.drop(h);
    }
    cb(err);
    if (this._opened) {
      // node emits it when libuv has closed the handle: after the jobs and ticks.
      this._opened = false;
      setImmediate(() => this.emit("close", Boolean(err)));
    } else {
      process.nextTick(() => this.emit("close", false));
    }
    const server = this._server;
    if (server !== null) {
      server._connections--;
      server._emitCloseIfDrained();
    }
  }

  destroySoon() {
    if (this.writable) this.end();
    if (this.writableFinished) this.destroy();
    else this.once("finish", this.destroy);
  }

  resetAndDestroy() {
    return this.destroy();
  }

  setTimeout(msecs, callback) {
    if (typeof msecs !== "number") throw $$errInvalidArgType("msecs", "of type number", msecs);
    this.timeout = msecs;
    this._clearTimer();
    if (msecs > 0) this._active();
    if (callback !== undefined) {
      if (typeof callback !== "function") throw $$errInvalidArgType("callback", "of type function", callback);
      if (msecs === 0) this.removeListener("timeout", callback);
      else this.once("timeout", callback);
    }
    return this;
  }

  _clearTimer() {
    if (this._timer !== null) {
      clearTimeout(this._timer);
      this._timer = null;
    }
  }

  // Activity: restart the idle timer.
  _active() {
    this._clearTimer();
    if (this.timeout > 0 && !this.destroyed) {
      this._timer = setTimeout(() => {
        this._timer = null;
        this.emit("timeout");
      }, this.timeout);
    }
  }

  setNoDelay() {
    // (`wasi:sockets` has no `TCP_NODELAY`.)
    return this;
  }

  setKeepAlive(enable = false) {
    this._keepAlive = Boolean(enable);
    if (this._handle !== null && !this.connecting) this._applyKeepAlive();
    return this;
  }

  _applyKeepAlive() {
    try {
      wasi.setKeepAliveEnabled(this._handle, this._keepAlive);
    } catch {
      // (Not supported: ignored, as node ignores a failed `setsockopt`.)
    }
  }

  ref() {
    return this;
  }

  unref() {
    return this;
  }
}

class Server extends EventEmitter {
  constructor(options, connectionListener) {
    super();
    if (typeof options === "function") {
      connectionListener = options;
      options = {};
    } else if (options == null || typeof options === "object") {
      options = { ...options };
    } else {
      throw $$errInvalidArgType("options", "of type object", options);
    }
    if (typeof connectionListener === "function") this.on("connection", connectionListener);
    this._connections = 0;
    this._handle = null;
    this._accepted = null;
    this._address = null;
    this.allowHalfOpen = options.allowHalfOpen || false;
    this.pauseOnConnect = Boolean(options.pauseOnConnect);
    this.noDelay = Boolean(options.noDelay);
    this.keepAlive = Boolean(options.keepAlive);
    this.maxConnections = undefined;
  }

  get listening() {
    return this._handle !== null;
  }

  listen(...args) {
    const normalized = normalizeArgs(args);
    let options = normalized[0];
    const cb = normalized[1];
    if (cb !== null) this.once("listening", cb);
    if (this._handle !== null) {
      const e = new Error("Listen method has been called more than once without closing.");
      e.code = "ERR_SERVER_ALREADY_LISTEN";
      throw e;
    }
    if (options.path != null) throw unavailableError("net.Server.listen({ path })", "no IPC sockets over WASI");
    if (options.handle !== undefined || options.fd !== undefined) {
      throw unavailableError("net.Server.listen({ handle })", "no handles over WASI");
    }
    // `listen(port, host, backlog)` (positional): only the port and the host.
    let port = options.port;
    if (port === undefined || port === null) port = 0;
    else port = validatePort(port, "options.port");
    const host = options.host;
    if (host === undefined || host === null || host === "") {
      // node listens on `::` if it can, else on `0.0.0.0`.
      if (!this._bind("::", 6, port, true)) this._bind("0.0.0.0", 4, port, false);
    } else if (isIP(host)) {
      this._bind(host, isIP(host), port, false);
    } else {
      lookupAll(host, 0, "verbatim").then(
        (addresses) => this._bind(addresses[0].address, addresses[0].family, port, false),
        (err) => process.nextTick(() => this.emit("error", err)),
      );
    }
    return this;
  }

  // Bind to `address`, listen and accept; `quiet`: a failure only returns false.
  _bind(address, family, port, quiet) {
    let h;
    let accepted;
    try {
      h = wasi.create(family === 6 ? "ipv6" : "ipv4");
      wasi.bind(h, socketAddress(address, port));
      accepted = wasi.listen(h);
    } catch (e) {
      if (h !== undefined) wasi.drop(h);
      if (quiet) return false;
      const err = errnoError(e, "listen", address, port, true);
      process.nextTick(() => this.emit("error", err));
      return false;
    }
    this._handle = h;
    this._accepted = accepted;
    this._address = addressInfo(wasi.getLocalAddress(h));
    process.nextTick(() => {
      if (this._handle === h) this.emit("listening");
    });
    this._accept(accepted);
    return true;
  }

  async _accept(accepted) {
    for await (const batch of accepted) {
      for (const h of batch) {
        if (this._handle === null ||
            (this.maxConnections !== undefined && this._connections >= this.maxConnections)) {
          wasi.drop(h);
          continue;
        }
        const socket = new Socket({ handle: h, allowHalfOpen: this.allowHalfOpen });
        if (this.keepAlive) socket.setKeepAlive(true);
        if (this.pauseOnConnect) socket.pause();
        socket.server = this;
        socket._server = this;
        this._connections++;
        this.emit("connection", socket);
      }
    }
  }

  address() {
    return this._address !== null ? { ...this._address } : null;
  }

  close(cb) {
    if (typeof cb === "function") {
      if (this._handle === null) {
        this.once("close", () => {
          const e = new Error("Server is not running.");
          e.code = "ERR_SERVER_NOT_RUNNING";
          cb(e);
        });
      } else {
        this.once("close", cb);
      }
    }
    if (this._handle !== null) {
      const accepted = this._accepted;
      this._accepted = null;
      accepted.cancel();
      accepted.return();
      wasi.drop(this._handle);
      this._handle = null;
      this._address = null;
    }
    this._emitCloseIfDrained();
    return this;
  }

  _emitCloseIfDrained() {
    if (this._handle !== null || this._connections > 0) return;
    process.nextTick(() => this.emit("close"));
  }

  getConnections(cb) {
    process.nextTick(cb, null, this._connections);
    return this;
  }

  ref() {
    return this;
  }

  unref() {
    return this;
  }

  [Symbol.asyncDispose]() {
    if (this._handle === null) return Promise.resolve();
    return new Promise((resolve, reject) => {
      this.close((err) => (err ? reject(err) : resolve()));
    });
  }
}

function createServer(options, connectionListener) {
  return new Server(options, connectionListener);
}

function connect(...args) {
  const normalized = normalizeArgs(args);
  const options = normalized[0];
  const socket = new Socket(options);
  if (options.timeout) socket.setTimeout(options.timeout);
  return socket.connect(normalized);
}

let autoSelectFamily = true;
let autoSelectFamilyAttemptTimeout = 250;

const WHY = "not over WASI sockets yet";

class Unavailable {
  constructor() {
    throw unavailableError(`net.${new.target.name}`, WHY);
  }
}

class BlockList extends Unavailable {}
class SocketAddress extends Unavailable {}
class BoundSocket extends Unavailable {}

module.exports = {
  _createServerHandle: fn("net", "_createServerHandle", WHY),
  _normalizeArgs: normalizeArgs,
  BlockList,
  SocketAddress,
  BoundSocket,
  connect,
  createConnection: connect,
  createServer,
  isIP,
  isIPv4,
  isIPv6,
  Server,
  Socket,
  Stream: Socket,
  getDefaultAutoSelectFamily() {
    return autoSelectFamily;
  },
  setDefaultAutoSelectFamily(value) {
    autoSelectFamily = Boolean(value);
  },
  getDefaultAutoSelectFamilyAttemptTimeout() {
    return autoSelectFamilyAttemptTimeout;
  },
  setDefaultAutoSelectFamilyAttemptTimeout(value) {
    autoSelectFamilyAttemptTimeout = value < 10 ? 10 : value;
  },
};
