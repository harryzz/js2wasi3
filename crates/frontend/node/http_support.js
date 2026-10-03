// What node 24's `_http_*` modules (T71, `scripts/gen-node-http.mjs`) take from node's internals that
// js2wasi3 doesn't have: async_hooks, perf hooks, diagnostics channels, debug logs and trace events are
// no-ops; `FreeList` is node's; timers are the globals.
"use strict";

const { codes: { ERR_INTERNAL_ASSERTION } } = require("node:internal/errors");

// internal/assert
function assert(value, message) {
  if (!value) throw new ERR_INTERNAL_ASSERTION(message);
}
assert.fail = (message) => {
  throw new ERR_INTERNAL_ASSERTION(message);
};

// internal/freelist
class FreeList {
  constructor(name, max, ctor) {
    this.name = name;
    this.ctor = ctor;
    this.max = max;
    this.list = [];
  }

  alloc() {
    return this.list.length > 0 ? this.list.pop() : Reflect.apply(this.ctor, this, arguments);
  }

  free(obj) {
    if (this.list.length < this.max) {
      this.list.push(obj);
      return true;
    }
    return false;
  }
}

// internal/util's `assignFunctionName`.
function assignFunctionName(name, fn) {
  if (typeof name !== "string") name = `[${name.description}]`;
  Object.defineProperty(fn, "name", { writable: false, enumerable: false, configurable: true, value: name });
  return fn;
}

const noop = () => {};
const channel = { hasSubscribers: false, publish: noop, subscribe: noop, unsubscribe: noop };
function debuglog() {
  const log = noop;
  log.enabled = false;
  return log;
}

module.exports = {
  assert,
  FreeList,
  assignFunctionName,
  kEmptyObject: Object.freeze({ __proto__: null }),
  // internal/options
  getOptionValue(name) {
    return name === "--max-http-header-size" ? 16384 : false;
  },
  // internal/async_hooks
  defaultTriggerAsyncIdScope(id, fn, ...args) {
    return fn(...args);
  },
  getOrSetAsyncId() {
    return 0;
  },
  symbols: { async_id_symbol: Symbol("async_id_symbol") },
  // internal/perf/observe
  hasObserver: () => false,
  startPerf: noop,
  stopPerf: noop,
  // diagnostics_channel
  channel: () => channel,
  // internal/util/debuglog
  debuglog,
  // trace_events, internal/constants
  getCategoryEnabledBuffer: () => new Uint8Array(1),
  trace: noop,
  CHAR_LOWERCASE_B: 98,
  CHAR_LOWERCASE_E: 101,
  // timers, internal/timers
  setInterval: (...args) => setInterval(...args),
  clearInterval: (t) => clearInterval(t),
  setImmediate: (...args) => setImmediate(...args),
  setUnrefTimeout(fn, ms) {
    const t = setTimeout(fn, ms);
    t.unref();
    return t;
  },
};
