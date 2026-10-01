// node's `events` (T55): `EventEmitter` from node 24's lib/events.js (MIT, Node.js contributors),
// without domains, `EventTarget`s, `AbortSignal`s, `events.on` and `EventEmitterAsyncResource`
// (they throw: F241). Warnings go through `process.emitWarning` (not printed, as node
// with `--no-warnings`).
"use strict";

const kCapture = Symbol("kCapture");
const kErrorMonitor = Symbol("events.errorMonitor");
const kShapeMode = Symbol("shapeMode");
const kMaxEventTargetListeners = Symbol("events.maxEventTargetListeners");
const kMaxEventTargetListenersWarned = Symbol("events.maxEventTargetListenersWarned");
const kRejection = Symbol.for("nodejs.rejection");

let defaultMaxListeners = 10;

function notImplemented(what) {
  return new Error(what + " isn't implemented in js2wasi3 (planned: F241)");
}

function validateNumber(value, name, min) {
  if (typeof value !== "number") throw $$errInvalidArgType(name, "of type number", value);
  if (value < min || value !== value) {
    throw $$errOutOfRange(name, ">= " + min, value);
  }
}

function checkListener(listener) {
  if (typeof listener !== "function") throw $$errInvalidArgType("listener", "of type function", listener);
}

function EventEmitter(opts) {
  EventEmitter.init.call(this, opts);
}
module.exports = EventEmitter;
// T61: node's, over any `AbortSignal` (here, without loading node:internal/abort: its listener
// option key is a registered symbol).
module.exports.addAbortListener = function addAbortListener(signal, listener) {
  const typeError = (message) => {
    const e = new TypeError(message);
    e.code = "ERR_INVALID_ARG_TYPE";
    return e;
  };
  if (signal === null || typeof signal !== "object" || !("aborted" in signal)) {
    throw typeError(`The "signal" argument must be an instance of AbortSignal. Received ${$$typeDescribe(signal)}`);
  }
  if (typeof listener !== "function") {
    throw typeError(`The "listener" argument must be of type function. Received ${$$typeDescribe(listener)}`);
  }
  let remove;
  if (signal.aborted) {
    queueMicrotask(() => listener());
  } else {
    signal.addEventListener("abort", listener, { once: true, [Symbol.for("js2wasi3.resistStopPropagation")]: true });
    remove = () => signal.removeEventListener("abort", listener);
  }
  return {
    __proto__: null,
    [Symbol.dispose]() {
      if (remove !== undefined) remove();
    },
  };
};
module.exports.once = once;
module.exports.on = function on(emitter, event, options) {
  throw notImplemented("events.on");
};
module.exports.getEventListeners = getEventListeners;
module.exports.getMaxListeners = getMaxListeners;
module.exports.listenerCount = listenerCount;
// Backwards-compat with node 0.10.x
EventEmitter.EventEmitter = EventEmitter;
EventEmitter.usingDomains = false;
EventEmitter.captureRejectionSymbol = kRejection;
Object.defineProperty(EventEmitter, "captureRejections", {
  get() {
    return EventEmitter.prototype[kCapture];
  },
  set(value) {
    if (typeof value !== "boolean") throw $$errInvalidArgType("EventEmitter.captureRejections", "of type boolean", value);
    EventEmitter.prototype[kCapture] = value;
  },
  enumerable: true,
});
// A getter, as in node (ES module imports read it: it can't throw).
function EventEmitterAsyncResource() {
  throw notImplemented("EventEmitterAsyncResource");
}
Object.defineProperty(EventEmitter, "EventEmitterAsyncResource", {
  enumerable: true,
  get() {
    return EventEmitterAsyncResource;
  },
  set: undefined,
  configurable: true,
});
EventEmitter.errorMonitor = kErrorMonitor;
Object.defineProperty(EventEmitter.prototype, kCapture, {
  value: false,
  writable: true,
  enumerable: false,
});
EventEmitter.prototype._events = undefined;
EventEmitter.prototype._eventsCount = 0;
EventEmitter.prototype._maxListeners = undefined;

Object.defineProperty(EventEmitter, "defaultMaxListeners", {
  enumerable: true,
  get: function () {
    return defaultMaxListeners;
  },
  set: function (arg) {
    if (typeof arg !== "number" || arg < 0 || arg !== arg) {
      throw $$errOutOfRange("defaultMaxListeners", "a non-negative number", arg);
    }
    defaultMaxListeners = arg;
  },
});
Object.defineProperty(EventEmitter, "kMaxEventTargetListeners", {
  value: kMaxEventTargetListeners,
  enumerable: false,
  configurable: false,
  writable: false,
});
Object.defineProperty(EventEmitter, "kMaxEventTargetListenersWarned", {
  value: kMaxEventTargetListenersWarned,
  enumerable: false,
  configurable: false,
  writable: false,
});

// Sets the max listeners of the given emitters (or the default when none is given).
EventEmitter.setMaxListeners = function (n = defaultMaxListeners, ...eventTargets) {
  validateNumber(n, "n", 0);
  if (eventTargets.length === 0) {
    defaultMaxListeners = n;
  } else {
    for (let i = 0; i < eventTargets.length; i++) {
      const target = eventTargets[i];
      if (typeof target.setMaxListeners === "function") {
        target.setMaxListeners(n);
      } else {
        throw $$errInvalidArgType("eventTargets", "an instance of EventEmitter or EventTarget", target);
      }
    }
  }
};

EventEmitter.init = function (opts) {
  if (this._events === undefined || this._events === Object.getPrototypeOf(this)._events) {
    this._events = { __proto__: null };
    this._eventsCount = 0;
    this[kShapeMode] = false;
  } else {
    this[kShapeMode] = true;
  }
  this._maxListeners = this._maxListeners || undefined;
  if (opts !== undefined && opts !== null && opts.captureRejections) {
    if (typeof opts.captureRejections !== "boolean") {
      throw $$errInvalidArgType("options.captureRejections", "of type boolean", opts.captureRejections);
    }
    this[kCapture] = Boolean(opts.captureRejections);
  } else {
    // Assigning the kCapture property directly saves an expensive timeout.
    this[kCapture] = EventEmitter.prototype[kCapture];
  }
};

function addCatch(that, promise, type, args) {
  if (!that[kCapture]) return;
  // Handle Promises/A+ spec, then could be a getter that throws on second use.
  try {
    const then = promise.then;
    if (typeof then === "function") {
      then.call(promise, undefined, function (err) {
        // The callback is called with nextTick to avoid a follow-up rejection from this promise.
        process.nextTick(emitUnhandledRejectionOrErr, that, err, type, args);
      });
    }
  } catch (err) {
    that.emit("error", err);
  }
}

function emitUnhandledRejectionOrErr(ee, err, type, args) {
  if (typeof ee[kRejection] === "function") {
    ee[kRejection](err, type, ...args);
  } else {
    // We have to disable the capture rejections mechanism, otherwise we might end up in an
    // infinite loop.
    const prev = ee[kCapture];
    try {
      ee[kCapture] = false;
      ee.emit("error", err);
    } finally {
      ee[kCapture] = prev;
    }
  }
}

EventEmitter.prototype.setMaxListeners = function setMaxListeners(n) {
  validateNumber(n, "setMaxListeners", 0);
  this._maxListeners = n;
  return this;
};

function _getMaxListeners(that) {
  if (that._maxListeners === undefined) return EventEmitter.defaultMaxListeners;
  return that._maxListeners;
}

EventEmitter.prototype.getMaxListeners = function getMaxListeners() {
  return _getMaxListeners(this);
};

EventEmitter.prototype.emit = function emit(type, ...args) {
  let doError = type === "error";
  const events = this._events;
  if (events !== undefined) {
    if (doError && events[kErrorMonitor] !== undefined) this.emit(kErrorMonitor, ...args);
    doError = doError && events.error === undefined;
  } else if (!doError) {
    return false;
  }
  // If there is no 'error' event listener then throw.
  if (doError) {
    let er;
    if (args.length > 0) er = args[0];
    if (er instanceof Error) {
      // Unhandled 'error' event
      throw er;
    }
    let stringifiedEr;
    try {
      stringifiedEr = __intrinsic_inspect(er, 0);
    } catch {
      stringifiedEr = er;
    }
    // At least give some kind of context to the user
    const err = new Error("Unhandled error. (" + stringifiedEr + ")");
    err.code = "ERR_UNHANDLED_ERROR";
    err.context = er;
    throw err;
  }
  const handler = events[type];
  if (handler === undefined) return false;
  if (typeof handler === "function") {
    const result = handler.apply(this, args);
    // We check if result is undefined first because that is the most common case so we do not
    // pay any perf penalty.
    if (result !== undefined && result !== null) addCatch(this, result, type, args);
  } else {
    const len = handler.length;
    const listeners = arrayClone(handler);
    for (let i = 0; i < len; ++i) {
      const result = listeners[i].apply(this, args);
      if (result !== undefined && result !== null) addCatch(this, result, type, args);
    }
  }
  return true;
};

function _addListener(target, type, listener, prepend) {
  let m;
  let events;
  let existing;
  checkListener(listener);
  events = target._events;
  if (events === undefined) {
    events = target._events = { __proto__: null };
    target._eventsCount = 0;
  } else {
    // To avoid recursion in the case that type === "newListener"! Before adding it to the
    // listeners, first emit "newListener".
    if (events.newListener !== undefined) {
      target.emit("newListener", type, listener.listener ?? listener);
      // Re-assign `events` because a newListener handler could have caused the this._events to
      // be assigned to a new object
      events = target._events;
    }
    existing = events[type];
  }
  if (existing === undefined) {
    // Optimize the case of one listener. Don't need the extra array object.
    events[type] = listener;
    ++target._eventsCount;
  } else {
    if (typeof existing === "function") {
      // Adding the second element, need to change to array.
      existing = events[type] = prepend ? [listener, existing] : [existing, listener];
      // If we've already got an array, just append.
    } else if (prepend) {
      existing.unshift(listener);
    } else {
      existing.push(listener);
    }
    // Check for listener leak
    m = _getMaxListeners(target);
    if (m > 0 && existing.length > m && !existing.warned) {
      existing.warned = true;
      const name = typeof target.constructor === "function" ? target.constructor.name : "Object";
      const w = new Error(
        "Possible EventEmitter memory leak detected. " + existing.length + " " + String(type) +
          " listeners added to [" + name + "]. MaxListeners is " + m +
          ". Use emitter.setMaxListeners() to increase limit",
      );
      w.name = "MaxListenersExceededWarning";
      w.emitter = target;
      w.type = type;
      w.count = existing.length;
      process.emitWarning(w);
    }
  }
  return target;
}

EventEmitter.prototype.addListener = function addListener(type, listener) {
  return _addListener(this, type, listener, false);
};

EventEmitter.prototype.on = EventEmitter.prototype.addListener;

EventEmitter.prototype.prependListener = function prependListener(type, listener) {
  return _addListener(this, type, listener, true);
};

function onceWrapper() {
  if (!this.fired) {
    this.target.removeListener(this.type, this.wrapFn);
    this.fired = true;
    if (arguments.length === 0) return this.listener.call(this.target);
    return this.listener.apply(this.target, arguments);
  }
}

function _onceWrap(target, type, listener) {
  const state = { fired: false, wrapFn: undefined, target, type, listener };
  const wrapped = onceWrapper.bind(state);
  wrapped.listener = listener;
  state.wrapFn = wrapped;
  return wrapped;
}

EventEmitter.prototype.once = function once(type, listener) {
  checkListener(listener);
  this.on(type, _onceWrap(this, type, listener));
  return this;
};

EventEmitter.prototype.prependOnceListener = function prependOnceListener(type, listener) {
  checkListener(listener);
  this.prependListener(type, _onceWrap(this, type, listener));
  return this;
};

EventEmitter.prototype.removeListener = function removeListener(type, listener) {
  checkListener(listener);
  const events = this._events;
  if (events === undefined) return this;
  const list = events[type];
  if (list === undefined) return this;
  if (list === listener || list.listener === listener) {
    this._eventsCount -= 1;
    if (this[kShapeMode]) {
      events[type] = undefined;
    } else if (this._eventsCount === 0) {
      this._events = { __proto__: null };
    } else {
      delete events[type];
      if (events.removeListener) this.emit("removeListener", type, list.listener || listener);
    }
  } else if (typeof list !== "function") {
    let position = -1;
    for (let i = list.length - 1; i >= 0; i--) {
      if (list[i] === listener || list[i].listener === listener) {
        position = i;
        break;
      }
    }
    if (position < 0) return this;
    if (position === 0) list.shift();
    else list.splice(position, 1);
    if (list.length === 1) events[type] = list[0];
    if (events.removeListener !== undefined) this.emit("removeListener", type, listener);
  }
  return this;
};

EventEmitter.prototype.off = EventEmitter.prototype.removeListener;

EventEmitter.prototype.removeAllListeners = function removeAllListeners(type) {
  const events = this._events;
  if (events === undefined) return this;
  // Not listening for removeListener, no need to emit
  if (events.removeListener === undefined) {
    if (arguments.length === 0) {
      this._events = { __proto__: null };
      this._eventsCount = 0;
    } else if (events[type] !== undefined) {
      if (--this._eventsCount === 0) this._events = { __proto__: null };
      else delete events[type];
    }
    this[kShapeMode] = false;
    return this;
  }
  // Emit removeListener for all listeners on all events
  if (arguments.length === 0) {
    const keys = Reflect.ownKeys(events);
    for (let i = 0; i < keys.length; i++) {
      const key = keys[i];
      if (key === "removeListener") continue;
      this.removeAllListeners(key);
    }
    this.removeAllListeners("removeListener");
    this._events = { __proto__: null };
    this._eventsCount = 0;
    this[kShapeMode] = false;
    return this;
  }
  const listeners = events[type];
  if (typeof listeners === "function") {
    this.removeListener(type, listeners);
  } else if (listeners !== undefined) {
    // LIFO order
    for (let i = listeners.length - 1; i >= 0; i--) {
      this.removeListener(type, listeners[i]);
    }
  }
  return this;
};

function _listeners(target, type, unwrap) {
  const events = target._events;
  if (events === undefined) return [];
  const evlistener = events[type];
  if (evlistener === undefined) return [];
  if (typeof evlistener === "function") return unwrap ? [evlistener.listener || evlistener] : [evlistener];
  return unwrap ? unwrapListeners(evlistener) : arrayClone(evlistener);
}

EventEmitter.prototype.listeners = function listeners(type) {
  return _listeners(this, type, true);
};

EventEmitter.prototype.rawListeners = function rawListeners(type) {
  return _listeners(this, type, false);
};

function listenerCount(emitter, type) {
  if (typeof emitter.listenerCount === "function") {
    return emitter.listenerCount(type);
  }
  return EventEmitter.prototype.listenerCount.call(emitter, type);
}

EventEmitter.prototype.listenerCount = function listenerCount(eventName, listener) {
  const events = this._events;
  if (events !== undefined) {
    const evlistener = events[eventName];
    if (typeof evlistener === "function") {
      if (listener != null) {
        return listener === evlistener || listener === evlistener.listener ? 1 : 0;
      }
      return 1;
    } else if (evlistener !== undefined) {
      if (listener != null) {
        let matching = 0;
        for (let i = 0, l = evlistener.length; i < l; i++) {
          if (evlistener[i] === listener || evlistener[i].listener === listener) matching++;
        }
        return matching;
      }
      return evlistener.length;
    }
  }
  return 0;
};

EventEmitter.prototype.eventNames = function eventNames() {
  return this._eventsCount > 0 ? Reflect.ownKeys(this._events) : [];
};

function arrayClone(arr) {
  // At least since V8 8.3, this implementation is faster than the previous which always used a
  // simple for-loop
  switch (arr.length) {
    case 2:
      return [arr[0], arr[1]];
    case 3:
      return [arr[0], arr[1], arr[2]];
    case 4:
      return [arr[0], arr[1], arr[2], arr[3]];
    case 5:
      return [arr[0], arr[1], arr[2], arr[3], arr[4]];
    case 6:
      return [arr[0], arr[1], arr[2], arr[3], arr[4], arr[5]];
  }
  return arr.slice();
}

function unwrapListeners(arr) {
  const ret = arrayClone(arr);
  for (let i = 0; i < ret.length; ++i) {
    const orig = ret[i].listener;
    if (typeof orig === "function") ret[i] = orig;
  }
  return ret;
}

// The listeners of an emitter (`EventTarget`s aren't implemented).
function getEventListeners(emitterOrTarget, type) {
  if (typeof emitterOrTarget.listeners === "function") {
    return emitterOrTarget.listeners(type);
  }
  throw $$errInvalidArgType("emitter", "an instance of EventEmitter or EventTarget", emitterOrTarget);
}

function getMaxListeners(emitterOrTarget) {
  if (typeof emitterOrTarget?.getMaxListeners === "function") {
    return _getMaxListeners(emitterOrTarget);
  }
  throw $$errInvalidArgType("emitter", "an instance of EventEmitter or EventTarget", emitterOrTarget);
}

// A promise for the next `name` event's arguments; an 'error' event rejects it.
async function once(emitter, name, options = {}) {
  if (options === null || typeof options !== "object") throw $$errInvalidArgType("options", "of type object", options);
  if (options.signal !== undefined) throw notImplemented("events.once with a signal");
  return new Promise((resolve, reject) => {
    const errorListener = (err) => {
      emitter.removeListener(name, resolver);
      reject(err);
    };
    const resolver = (...args) => {
      if (typeof emitter.removeListener === "function") {
        emitter.removeListener("error", errorListener);
      }
      resolve(args);
    };
    if (typeof emitter.once !== "function") {
      throw $$errInvalidArgType("emitter", "an instance of EventEmitter or EventTarget", emitter);
    }
    emitter.once(name, resolver);
    if (name !== "error") emitter.once("error", errorListener);
  });
}
