// node:internal/abort (T60): a minimal `Event`, `EventTarget`, `AbortSignal` and `AbortController`
// for `node:stream` (readable-stream needs them: `pipeline` makes an `AbortController` per call).
// Not node's globals yet (F241): only the stream built-ins use them. Simplified from node's
// lib/internal/event_target.js and abort_controller.js: no capture/bubbling phases, `passive`,
// `isTrusted`, or listener leak warnings.
'use strict';

const kEvents = Symbol('kEvents');
const kStop = Symbol('kStop');

class Event {
  constructor(...args) {
    const type = args[0];
    const options = args[1];
    if (args.length === 0) {
      const e = new TypeError('The "type" argument must be specified');
      e.code = 'ERR_MISSING_ARGS';
      throw e;
    }
    this.type = `${type}`;
    this.bubbles = !!(options && options.bubbles);
    this.cancelable = !!(options && options.cancelable);
    this.defaultPrevented = false;
    this.target = null;
    this.currentTarget = null;
    this.timeStamp = 0;
    this[kStop] = false;
  }
  preventDefault() {
    if (this.cancelable) this.defaultPrevented = true;
  }
  stopImmediatePropagation() {
    this[kStop] = true;
  }
  stopPropagation() {}
}

class EventTarget {
  constructor() {
    this[kEvents] = new Map();
  }
  addEventListener(type, listener, options) {
    if (listener === null || listener === undefined) return;
    const once = typeof options === 'object' && options !== null && !!options.once;
    const signal = typeof options === 'object' && options !== null ? options.signal : undefined;
    if (signal !== undefined && signal.aborted) return;
    let list = this[kEvents].get(type);
    if (list === undefined) {
      list = [];
      this[kEvents].set(type, list);
    }
    for (const l of list) if (l.listener === listener) return;
    const entry = { listener, once, removed: false };
    list.push(entry);
    if (signal !== undefined) {
      signal.addEventListener('abort', () => this.removeEventListener(type, listener), { once: true });
    }
  }
  removeEventListener(type, listener) {
    const list = this[kEvents].get(type);
    if (list === undefined) return;
    for (let i = 0; i < list.length; i++) {
      if (list[i].listener === listener) {
        list[i].removed = true;
        list.splice(i, 1);
        return;
      }
    }
  }
  dispatchEvent(event) {
    if (!(event instanceof Event)) {
      const e = new TypeError('The "event" argument must be an instance of Event. Received ' + typeof event);
      e.code = 'ERR_INVALID_ARG_TYPE';
      throw e;
    }
    event.target = this;
    event.currentTarget = this;
    const list = this[kEvents].get(event.type);
    if (list !== undefined) {
      for (const entry of list.slice()) {
        if (entry.removed) continue;
        if (entry.once) this.removeEventListener(event.type, entry.listener);
        const l = entry.listener;
        try {
          if (typeof l === 'function') l.call(this, event);
          else if (l !== null && typeof l.handleEvent === 'function') l.handleEvent(event);
        } catch (err) {
          // node reports a listener's error as uncaught on the next tick.
          process.nextTick(() => {
            throw err;
          });
        }
        if (event[kStop]) break;
      }
    }
    event.currentTarget = null;
    return !event.defaultPrevented;
  }
}

const kAborted = Symbol('kAborted');
const kReason = Symbol('kReason');
const kCreate = Symbol('kCreate');
const kSignal = Symbol('kSignal');

function illegal() {
  const e = new TypeError('Illegal constructor');
  e.code = 'ERR_ILLEGAL_CONSTRUCTOR';
  return e;
}

class AbortSignal extends EventTarget {
  constructor(key) {
    if (key !== kCreate) throw illegal();
    super();
    this[kAborted] = false;
    this[kReason] = undefined;
    this.onabort = null;
  }
  get aborted() {
    return this[kAborted];
  }
  get reason() {
    return this[kReason];
  }
  throwIfAborted() {
    if (this[kAborted]) throw this[kReason];
  }
  static abort(reason) {
    const s = new AbortSignal(kCreate);
    s[kAborted] = true;
    s[kReason] = reason === undefined ? abortError() : reason;
    return s;
  }
  static timeout(delay) {
    const s = new AbortSignal(kCreate);
    setTimeout(() => abortSignal(s, $$domException('The operation was aborted due to timeout', 'TimeoutError')), delay);
    return s;
  }
  static any(signals) {
    const s = new AbortSignal(kCreate);
    for (const t of signals) {
      if (t.aborted) {
        s[kAborted] = true;
        s[kReason] = t.reason;
        return s;
      }
    }
    for (const t of signals) t.addEventListener('abort', () => abortSignal(s, t.reason), { once: true });
    return s;
  }
}

function abortError() {
  return $$domException('This operation was aborted', 'AbortError');
}

function abortSignal(signal, reason) {
  if (signal[kAborted]) return;
  signal[kAborted] = true;
  signal[kReason] = reason;
  const event = new Event('abort');
  if (typeof signal.onabort === 'function') {
    event.target = signal;
    event.currentTarget = signal;
    signal.onabort.call(signal, event);
  }
  signal.dispatchEvent(event);
}

class AbortController {
  constructor() {
    this[kSignal] = new AbortSignal(kCreate);
  }
  get signal() {
    return this[kSignal];
  }
  abort(reason) {
    abortSignal(this[kSignal], reason === undefined ? abortError() : reason);
  }
}

// node's internal/event_target symbols (listener options the streams pass; ignored here).
const kWeakHandler = Symbol('kWeak');
const kResistStopPropagation = Symbol('kResistStopPropagation');

module.exports = { Event, EventTarget, AbortSignal, AbortController, kWeakHandler, kResistStopPropagation };
