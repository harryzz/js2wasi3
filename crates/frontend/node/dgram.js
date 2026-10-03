// node's `dgram` (T67): UDP over `wasi:sockets`. `Socket` follows node 24's lib/dgram.js (bind and
// connect states, the queue of sends made while binding, its errors) over `UDP`, a handle shaped
// like node's `udp_wrap` whose calls are `wasi:sockets` `udp-socket`'s: `bind`/`connect` are
// synchronous, `send` and `receive` are `async func`s (a receive loop is one pending `receive`;
// `close` cancels it before dropping the socket). Multicast, broadcast and buffer sizes aren't
// there (WASI 0.3 has none of them), nor cluster or `BlockList`s.
"use strict";

const EventEmitter = require("events");
const { Buffer } = require("buffer");
const udp = require("node:internal/wasi_udp");
const { isIP, socketAddress, addressInfo, wasiCode, errnoError, lookupAll } = require("node:internal/net");
const { unavailableError } = require("node:internal/unavailable");

const BIND_STATE_UNBOUND = 0;
const BIND_STATE_BINDING = 1;
const BIND_STATE_BOUND = 2;

const CONNECT_STATE_DISCONNECTED = 0;
const CONNECT_STATE_CONNECTING = 1;
const CONNECT_STATE_CONNECTED = 2;

const kStateSymbol = Symbol("state symbol");

function nodeError(Ctor, code, message) {
  const e = new Ctor(message);
  e.code = code;
  return e;
}

const errBadType = () =>
  nodeError(TypeError, "ERR_SOCKET_BAD_TYPE", "Bad socket type specified. Valid types are: udp4, udp6");
const errAlreadyBound = () => nodeError(Error, "ERR_SOCKET_ALREADY_BOUND", "Socket is already bound");
const errNotRunning = () => nodeError(Error, "ERR_SOCKET_DGRAM_NOT_RUNNING", "Not running");
const errIsConnected = () => nodeError(Error, "ERR_SOCKET_DGRAM_IS_CONNECTED", "Already connected");
const errNotConnected = () => nodeError(Error, "ERR_SOCKET_DGRAM_NOT_CONNECTED", "Not connected");
const errOutOfBounds = (name) =>
  nodeError(RangeError, "ERR_BUFFER_OUT_OF_BOUNDS", `"${name}" is outside of buffer bounds`);
const BUFFER_TYPES = "of type string or an instance of Buffer, TypedArray, or DataView";

// node's `validatePort` (lib/internal/validators.js).
function validatePort(port, name = "Port", allowZero = true) {
  if ((typeof port !== "number" && typeof port !== "string") ||
      (typeof port === "string" && port.trim().length === 0) ||
      +port !== (+port >>> 0) ||
      port > 0xFFFF ||
      (port === 0 && !allowZero)) {
    throw nodeError(RangeError, "ERR_SOCKET_BAD_PORT",
      `${name} should be ${allowZero ? ">=" : ">"} 0 and < 65536. Received ${$$nodeSpecificType(port)}.`);
  }
  return port | 0;
}

function validateString(v, name) {
  if (typeof v !== "string") throw $$errInvalidArgType(name, "of type string", v);
}

function validateNumber(v, name) {
  if (typeof v !== "number") throw $$errInvalidArgType(name, "of type number", v);
}

// libuv's errors that aren't `wasi:sockets` codes.
const UV = {
  EBADF: [-9, "bad file descriptor"],
  ENOTCONN: [-107, "socket is not connected"],
  ENOSYS: [-38, "function not implemented"],
};

// node's `ErrnoException` (`getsockname EBADF`) of a `wasi:sockets` error or a libuv code.
function errnoException(err, syscall) {
  if (typeof err === "string" && UV[err]) {
    const e = new Error(`${syscall} ${err}`);
    e.errno = UV[err][0];
    e.code = err;
    e.syscall = syscall;
    return e;
  }
  return errnoError(err, syscall);
}

const isArrayBufferView = (v) => ArrayBuffer.isView(v);

// node's `udp_wrap` `UDP` handle over a `udp-socket`. Methods return 0 or an error (a
// `wasi:sockets` error code, or a libuv code for what WASI doesn't report).
class UDP {
  constructor(type) {
    this.type = type;
    this.socket = udp.create(type === "udp4" ? "ipv4" : "ipv6");
    this.bound = false;
    this.connected = false;
    this.receiving = undefined;
    this.sending = 0;
    this.closed = false;
    this.onmessage = undefined;
    this.onerror = undefined;
  }

  // `internal/dgram`'s `lookup4`/`lookup6`: `(err, ip)`, after the current operation.
  lookup(address, callback) {
    const family = this.type === "udp4" ? 4 : 6;
    if (!address) address = family === 4 ? "127.0.0.1" : "::1";
    if (isIP(address)) {
      process.nextTick(callback, null, address);
      return;
    }
    lookupAll(address, family, "verbatim").then(
      (all) => callback(null, all[0].address),
      (err) => callback(err),
    );
  }

  bind(ip, port) {
    try {
      udp.bind(this.socket, socketAddress(ip, port));
    } catch (e) {
      return wasiCode(e);
    }
    this.bound = true;
    return 0;
  }

  connect(ip, port) {
    try {
      udp.connect(this.socket, socketAddress(ip, port));
    } catch (e) {
      return wasiCode(e);
    }
    this.connected = true;
    return 0;
  }

  disconnect() {
    try {
      udp.disconnect(this.socket);
    } catch (e) {
      return wasiCode(e);
    }
    this.connected = false;
    return 0;
  }

  // `send(req, list, count[, port, ip], hasCallback)`: the chunks as one datagram; `req.oncomplete(err,
  // bytes)` when it's sent. 0 (the result comes later).
  send(req, list, count, port, ip) {
    let size = 0;
    for (let i = 0; i < count; i++) size += list[i].length;
    const data = new Uint8Array(size);
    let at = 0;
    for (let i = 0; i < count; i++) {
      data.set(list[i], at);
      at += list[i].length;
    }
    const to = typeof port === "number" ? socketAddress(ip, port) : undefined;
    this.sending++;
    udp.send(this.socket, data, to).then(
      () => this.sent(req, 0, size),
      (e) => this.sent(req, wasiCode(e), 0),
    );
    return 0;
  }

  sent(req, err, size) {
    this.sending--;
    if (this.closed && this.sending === 0 && this.receiving === undefined) this.drop();
    if (req.oncomplete) req.oncomplete(err, size);
  }

  recvStart() {
    if (this.receiving !== undefined || this.closed) return;
    const p = udp.receive(this.socket);
    this.receiving = p;
    p.then(([data, from]) => {
      if (this.receiving !== p) return;
      this.receiving = undefined;
      const rinfo = addressInfo(from);
      rinfo.size = data.length;
      this.recvStart();
      if (this.onmessage) this.onmessage(data.length, this, Buffer.from(data.buffer, data.byteOffset, data.length), rinfo);
    }, (e) => {
      if (this.receiving !== p) return;
      this.receiving = undefined;
      if (this.onerror) this.onerror(0, this, errnoError(e, "recvmsg"));
    });
  }

  recvStop() {
    const p = this.receiving;
    if (p === undefined) return;
    this.receiving = undefined;
    // A pending `receive` borrows the socket: cancel it first.
    udp.cancel(p);
  }

  getsockname(out) {
    if (!this.bound) return "EBADF";
    let sa;
    try {
      sa = udp.getLocalAddress(this.socket);
    } catch (e) {
      return wasiCode(e);
    }
    Object.assign(out, addressInfo(sa));
    return 0;
  }

  getpeername(out) {
    if (!this.connected) return "ENOTCONN";
    let sa;
    try {
      sa = udp.getRemoteAddress(this.socket);
    } catch (e) {
      return wasiCode(e);
    }
    Object.assign(out, addressInfo(sa));
    return 0;
  }

  setTTL(ttl) {
    try {
      udp.setUnicastHopLimit(this.socket, ttl);
    } catch (e) {
      return wasiCode(e);
    }
    return 0;
  }

  close() {
    this.recvStop();
    this.closed = true;
    if (this.sending === 0) this.drop();
  }

  drop() {
    if (this.socket === undefined) return;
    udp.drop(this.socket);
    this.socket = undefined;
  }
}

function newHandle(type) {
  if (type === "udp4" || type === "udp6") return new UDP(type);
  throw errBadType();
}

class Socket extends EventEmitter {
  constructor(type, listener) {
    super();
    let options;
    if (type !== null && typeof type === "object") {
      options = type;
      type = options.type;
    }
    const handle = newHandle(type);
    this.type = type;
    if (typeof listener === "function") this.on("message", listener);
    this[kStateSymbol] = {
      handle,
      receiving: false,
      bindState: BIND_STATE_UNBOUND,
      connectState: CONNECT_STATE_DISCONNECTED,
      queue: undefined,
      reuseAddr: options?.reuseAddr,
      reusePort: options?.reusePort,
      ipv6Only: options?.ipv6Only,
    };
    if (options?.signal !== undefined) {
      const { signal } = options;
      const onAborted = () => {
        if (this[kStateSymbol].handle) this.close();
      };
      if (signal.aborted) onAborted();
      else {
        signal.addEventListener("abort", onAborted, { once: true });
        this.once("close", () => signal.removeEventListener("abort", onAborted));
      }
    }
  }

  bind(port_, address_ /* , callback */) {
    let port = port_;
    healthCheck(this);
    const state = this[kStateSymbol];
    if (state.bindState !== BIND_STATE_UNBOUND) throw errAlreadyBound();
    state.bindState = BIND_STATE_BINDING;

    const cb = arguments.length && arguments[arguments.length - 1];
    if (typeof cb === "function") {
      const removeListeners = () => {
        this.removeListener(EventEmitter.errorMonitor, removeListeners);
        this.removeListener("listening", onListening);
      };
      const onListening = () => {
        removeListeners();
        cb.call(this);
      };
      this.on(EventEmitter.errorMonitor, removeListeners);
      this.on("listening", onListening);
    }

    let address;
    if (port !== null && typeof port === "object") {
      if (typeof port.fd === "number" && port.fd > 0) {
        throw unavailableError("dgram.Socket.bind({ fd })", "WASI has no file descriptors for sockets");
      }
      address = port.address || "";
      port = port.port;
    } else {
      address = typeof address_ === "function" ? "" : address_;
    }
    if (!address) address = this.type === "udp4" ? "0.0.0.0" : "::";

    state.handle.lookup(address, (err, ip) => {
      if (!state.handle) return; // closed in the meantime
      if (err) {
        state.bindState = BIND_STATE_UNBOUND;
        this.emit("error", err);
        return;
      }
      const e = state.handle.bind(ip, port || 0);
      if (e) {
        const ex = errnoError(e, "bind", ip, port);
        state.bindState = BIND_STATE_UNBOUND;
        this.emit("error", ex);
        return;
      }
      startListening(this);
    });
    return this;
  }

  bindSync(options = {}) {
    healthCheck(this);
    if (options === null || typeof options !== "object") throw $$errInvalidArgType("options", "of type object", options);
    const state = this[kStateSymbol];
    if (state.bindState !== BIND_STATE_UNBOUND) throw errAlreadyBound();
    let { port = 0, address } = options;
    port = validatePort(port, "options.port");
    if (address == null || address === "") address = this.type === "udp4" ? "0.0.0.0" : "::";
    else {
      validateString(address, "options.address");
      if (isIP(address) === 0) {
        const e = new TypeError(`The property 'options.address' must be a numeric IP address; bindSync does not perform DNS resolution. Received '${address}'`);
        e.code = "ERR_INVALID_ARG_VALUE";
        throw e;
      }
    }
    const err = state.handle.bind(address, port);
    if (err) throw errnoError(err, "bind", address, port);
    startReceiving(this);
    process.nextTick(emitListeningNT, this);
    return this.address();
  }

  connect(port, address, callback) {
    port = validatePort(port, "Port", false);
    if (typeof address === "function") {
      callback = address;
      address = "";
    } else if (address === undefined) {
      address = "";
    }
    validateString(address, "address");
    const state = this[kStateSymbol];
    if (state.connectState !== CONNECT_STATE_DISCONNECTED) throw errIsConnected();
    state.connectState = CONNECT_STATE_CONNECTING;
    if (state.bindState === BIND_STATE_UNBOUND) this.bind({ port: 0, exclusive: true }, null);
    if (state.bindState !== BIND_STATE_BOUND) {
      enqueue(this, _connect.bind(this, port, address, callback));
      return;
    }
    _connect.call(this, port, address, callback);
  }

  connectSync(port, address) {
    healthCheck(this);
    port = validatePort(port, "Port", false);
    const state = this[kStateSymbol];
    if (state.connectState !== CONNECT_STATE_DISCONNECTED) throw errIsConnected();
    if (address == null || address === "") address = this.type === "udp4" ? "127.0.0.1" : "::1";
    else {
      validateString(address, "address");
      if (isIP(address) === 0) {
        const e = new TypeError(`The argument 'address' must be a numeric IP address; connectSync does not perform DNS resolution. Received '${address}'`);
        e.code = "ERR_INVALID_ARG_VALUE";
        throw e;
      }
    }
    if (state.bindState === BIND_STATE_UNBOUND) this.bindSync();
    else if (state.bindState !== BIND_STATE_BOUND) throw errAlreadyBound();
    state.connectState = CONNECT_STATE_CONNECTING;
    const err = state.handle.connect(address, port);
    if (err) {
      state.connectState = CONNECT_STATE_DISCONNECTED;
      throw errnoError(err, "connect", address, port);
    }
    state.connectState = CONNECT_STATE_CONNECTED;
    process.nextTick(emitConnectNT, this);
  }

  disconnect() {
    const state = this[kStateSymbol];
    if (state.connectState !== CONNECT_STATE_CONNECTED) throw errNotConnected();
    const err = state.handle.disconnect();
    if (err) throw errnoException(err, "connect");
    state.connectState = CONNECT_STATE_DISCONNECTED;
  }

  sendto(buffer, offset, length, port, address, callback) {
    validateNumber(offset, "offset");
    validateNumber(length, "length");
    validateNumber(port, "port");
    validateString(address, "address");
    this.send(buffer, offset, length, port, address, callback);
  }

  send(buffer, offset, length, port, address, callback) {
    let list;
    const state = this[kStateSymbol];
    const connected = state.connectState === CONNECT_STATE_CONNECTED;
    if (!connected) {
      if (address || (port && typeof port !== "function")) {
        buffer = sliceBuffer(buffer, offset, length);
      } else {
        callback = port;
        port = offset;
        address = length;
      }
    } else {
      if (typeof length === "number") {
        buffer = sliceBuffer(buffer, offset, length);
        if (typeof port === "function") {
          callback = port;
          port = null;
        }
      } else {
        callback = offset;
      }
      if (port || address) throw errIsConnected();
    }

    if (!Array.isArray(buffer)) {
      if (typeof buffer === "string") list = [Buffer.from(buffer)];
      else if (!isArrayBufferView(buffer)) throw $$errInvalidArgType("buffer", BUFFER_TYPES, buffer);
      else list = [buffer];
    } else if (!(list = fixBufferList(buffer))) {
      throw $$errInvalidArgType("buffer list arguments", BUFFER_TYPES, buffer);
    }

    if (!connected) port = validatePort(port, "Port", false);
    if (typeof callback !== "function") callback = undefined;
    if (typeof address === "function") {
      callback = address;
      address = undefined;
    } else if (address != null) {
      validateString(address, "address");
    }

    healthCheck(this);
    if (state.bindState === BIND_STATE_UNBOUND) this.bind({ port: 0, exclusive: true }, null);
    if (list.length === 0) list.push(Buffer.alloc(0));
    if (state.bindState !== BIND_STATE_BOUND) {
      enqueue(this, this.send.bind(this, list, port, address, callback));
      return;
    }
    const afterDns = (ex, ip) => doSend(ex, this, ip, list, address, port, callback);
    if (!connected) state.handle.lookup(address, afterDns);
    else afterDns(null, null);
  }

  close(callback) {
    const state = this[kStateSymbol];
    const queue = state.queue;
    if (typeof callback === "function") this.on("close", callback);
    if (queue !== undefined) {
      queue.push(this.close.bind(this));
      return this;
    }
    healthCheck(this);
    stopReceiving(this);
    state.handle.close();
    state.handle = null;
    process.nextTick(socketCloseNT, this);
    return this;
  }

  async [Symbol.asyncDispose]() {
    if (!this[kStateSymbol].handle) return;
    await new Promise((resolve) => this.close(resolve));
  }

  address() {
    healthCheck(this);
    const out = {};
    const err = this[kStateSymbol].handle.getsockname(out);
    if (err) throw errnoException(err, "getsockname");
    return out;
  }

  remoteAddress() {
    healthCheck(this);
    const state = this[kStateSymbol];
    if (state.connectState !== CONNECT_STATE_CONNECTED) throw errNotConnected();
    const out = {};
    const err = state.handle.getpeername(out);
    if (err) throw errnoException(err, "getpeername");
    return out;
  }

  setTTL(ttl) {
    validateNumber(ttl, "ttl");
    healthCheck(this);
    const err = this[kStateSymbol].handle.setTTL(ttl);
    if (err) throw errnoException(err, "setTTL");
    return ttl;
  }

  // Not over WASI 0.3 (no broadcast or multicast options, no buffer sizes): libuv's ENOSYS.
  setBroadcast() {
    healthCheck(this);
    throw errnoException("ENOSYS", "setBroadcast");
  }

  setMulticastTTL() {
    healthCheck(this);
    throw errnoException("ENOSYS", "setMulticastTTL");
  }

  setMulticastLoopback() {
    healthCheck(this);
    throw errnoException("ENOSYS", "setMulticastLoopback");
  }

  setMulticastInterface() {
    healthCheck(this);
    throw errnoException("ENOSYS", "setMulticastInterface");
  }

  addMembership() {
    healthCheck(this);
    throw errnoException("ENOSYS", "addMembership");
  }

  dropMembership() {
    healthCheck(this);
    throw errnoException("ENOSYS", "dropMembership");
  }

  addSourceSpecificMembership() {
    healthCheck(this);
    throw errnoException("ENOSYS", "addSourceSpecificMembership");
  }

  dropSourceSpecificMembership() {
    healthCheck(this);
    throw errnoException("ENOSYS", "dropSourceSpecificMembership");
  }

  // (Like `net`'s, F289: the program waits for a bound socket either way.)
  ref() {
    return this;
  }

  unref() {
    return this;
  }

  setRecvBufferSize() {
    throw bufferSizeError("set", "recv");
  }

  setSendBufferSize() {
    throw bufferSizeError("set", "send");
  }

  getRecvBufferSize() {
    throw bufferSizeError("get", "recv");
  }

  getSendBufferSize() {
    throw bufferSizeError("get", "send");
  }

  getSendQueueSize() {
    return 0;
  }

  getSendQueueCount() {
    return this[kStateSymbol].handle ? this[kStateSymbol].handle.sending : 0;
  }

  _healthCheck() {
    healthCheck(this);
  }

  _stopReceiving() {
    stopReceiving(this);
  }
}

// node's `_handle`, `_receiving`, … accessors (deprecated aliases of the state).
for (const [name, key] of [["_handle", "handle"], ["_receiving", "receiving"], ["_bindState", "bindState"],
  ["_queue", "queue"], ["_reuseAddr", "reuseAddr"]]) {
  Object.defineProperty(Socket.prototype, name, {
    get() { return this[kStateSymbol][key]; },
    set(v) { this[kStateSymbol][key] = v; },
    configurable: true,
  });
}

function bufferSizeError(op, which) {
  const e = new Error(`Could not ${op} or ${op === "get" ? "set" : "get"} buffer size: uv_${which}_buffer_size returned ENOSYS (function not implemented)`);
  e.code = "ERR_SOCKET_BUFFER_SIZE";
  return e;
}

function createSocket(type, listener) {
  return new Socket(type, listener);
}

function startReceiving(socket) {
  const state = socket[kStateSymbol];
  state.handle.onmessage = onMessage;
  state.handle.onerror = onError;
  state.handle.owner = socket;
  state.handle.recvStart();
  state.receiving = true;
  state.bindState = BIND_STATE_BOUND;
}

function startListening(socket) {
  startReceiving(socket);
  socket.emit("listening");
}

function emitListeningNT(socket) {
  if (socket[kStateSymbol].handle) socket.emit("listening");
}

function emitConnectNT(socket) {
  if (socket[kStateSymbol].handle) socket.emit("connect");
}

function _connect(port, address, callback) {
  const state = this[kStateSymbol];
  if (callback) this.once("connect", callback);
  state.handle.lookup(address, (ex, ip) => doConnect(ex, this, ip, address, port, callback));
}

function doConnect(ex, self, ip, address, port, callback) {
  const state = self[kStateSymbol];
  if (!state.handle) return;
  if (!ex) {
    const err = state.handle.connect(ip, port);
    if (err) ex = errnoError(err, "connect", address, port);
  }
  if (ex) {
    state.connectState = CONNECT_STATE_DISCONNECTED;
    return process.nextTick(() => {
      if (callback) {
        self.removeListener("connect", callback);
        callback(ex);
      } else {
        self.emit("error", ex);
      }
    });
  }
  state.connectState = CONNECT_STATE_CONNECTED;
  process.nextTick(() => self.emit("connect"));
}

function sliceBuffer(buffer, offset, length) {
  if (typeof buffer === "string") buffer = Buffer.from(buffer);
  else if (!isArrayBufferView(buffer)) throw $$errInvalidArgType("buffer", BUFFER_TYPES, buffer);
  offset = offset >>> 0;
  length = length >>> 0;
  if (offset > buffer.byteLength) throw errOutOfBounds("offset");
  if (offset + length > buffer.byteLength) throw errOutOfBounds("length");
  return Buffer.from(buffer.buffer, buffer.byteOffset + offset, length);
}

function fixBufferList(list) {
  const newlist = new Array(list.length);
  for (let i = 0, l = list.length; i < l; i++) {
    const buf = list[i];
    if (typeof buf === "string") newlist[i] = Buffer.from(buf);
    else if (Buffer.isBuffer(buf)) newlist[i] = buf;
    else if (!isArrayBufferView(buf)) return null;
    else newlist[i] = Buffer.from(buf.buffer, buf.byteOffset, buf.byteLength);
  }
  return newlist;
}

function enqueue(self, toEnqueue) {
  const state = self[kStateSymbol];
  if (state.queue === undefined) {
    state.queue = [];
    self.once(EventEmitter.errorMonitor, onListenError);
    self.once("listening", onListenSuccess);
  }
  state.queue.push(toEnqueue);
}

function onListenSuccess() {
  this.removeListener(EventEmitter.errorMonitor, onListenError);
  clearQueue.call(this);
}

function onListenError() {
  this.removeListener("listening", onListenSuccess);
  this[kStateSymbol].queue = undefined;
}

function clearQueue() {
  const state = this[kStateSymbol];
  const queue = state.queue;
  state.queue = undefined;
  for (const entry of queue) entry();
}

function doSend(ex, self, ip, list, address, port, callback) {
  const state = self[kStateSymbol];
  if (ex) {
    if (typeof callback === "function") {
      process.nextTick(callback, ex);
      return;
    }
    process.nextTick(() => self.emit("error", ex));
    return;
  } else if (!state.handle) {
    return;
  }
  const req = { list, address, port, callback: undefined, oncomplete: undefined };
  if (callback) {
    req.callback = callback;
    req.oncomplete = afterSend;
  }
  let err;
  if (port) err = state.handle.send(req, list, list.length, port, ip, !!callback);
  else err = state.handle.send(req, list, list.length, !!callback);
  if (err && callback) process.nextTick(callback, errnoError(err, "send", address, port));
}

function afterSend(err, sent) {
  this.callback(err ? errnoError(err, "send", this.address, this.port) : null, sent);
}

function socketCloseNT(self) {
  self.emit("close");
}

function healthCheck(socket) {
  if (!socket[kStateSymbol].handle) throw errNotRunning();
}

function stopReceiving(socket) {
  const state = socket[kStateSymbol];
  if (!state.receiving) return;
  state.handle.recvStop();
  state.receiving = false;
}

function onMessage(nread, handle, buf, rinfo) {
  const self = handle.owner;
  return self.emit("message", buf, rinfo);
}

function onError(nread, handle, error) {
  const self = handle.owner;
  return self.emit("error", error);
}

function _createSocketHandle(address, port, addressType) {
  const handle = newHandle(addressType);
  if (port || address) {
    const err = handle.bind(address, port || 0);
    if (err) {
      handle.close();
      return wasiCode(err);
    }
  }
  return handle;
}

module.exports = {
  _createSocketHandle,
  createSocket,
  Socket,
};
