// node:internal/abort (T60, T61): `Event`, `EventTarget`, `AbortSignal`, `AbortController`: node's
// globals (bound by the linker, `node::GLOBALS`) and what `node:stream` uses. After node 24's
// lib/internal/event_target.js and abort_controller.js (MIT), simplified: no `CustomEvent`,
// `NodeEventTarget`, weak listeners, listener leak warnings, or transfer between threads.
'use strict';

const inspectCustom = Symbol.for('nodejs.util.inspect.custom');

function codeError(Base, code, message) {
  const e = new Base(message);
  e.code = code;
  return e;
}

function invalidThis(type) {
  return codeError(TypeError, 'ERR_INVALID_THIS', `Value of "this" must be of type ${type}`);
}

const kTrust = Symbol('kTrust');

class Event {
  #type;
  #cancelable = false;
  #bubbles = false;
  #composed = false;
  #defaultPrevented = false;
  #timestamp = performance.now();
  #propagationStopped = false;
  #stop = false;
  #trusted = false;
  // node's kTarget/kIsBeingDispatched.
  #target = null;
  #dispatching = false;

  constructor(...args) {
    if (args.length === 0) throw codeError(TypeError, 'ERR_MISSING_ARGS', 'The "type" argument must be specified');
    const options = args[1];
    if (options !== undefined && options !== null && typeof options !== 'object') {
      throw codeError(TypeError, 'ERR_INVALID_ARG_TYPE',
        `The "options" argument must be of type object. Received ${$$typeDescribe(options)}`);
    }
    this.#type = `${args[0]}`;
    this.#bubbles = !!(options && options.bubbles);
    this.#cancelable = !!(options && options.cancelable);
    this.#composed = !!(options && options.composed);
    if (options && options[kTrust]) this.#trusted = true;
  }

  static is(v) {
    return typeof v === 'object' && v !== null && #type in v;
  }

  static setDispatch(e, target, dispatching) {
    if (target !== undefined) e.#target = target;
    e.#dispatching = dispatching;
  }

  static stopped(e) {
    return e.#stop;
  }

  initEvent(type, bubbles = false, cancelable = false) {
    if (!isEvent(this)) throw invalidThis('Event');
    if (this.#dispatching) return;
    this.#type = `${type}`;
    this.#bubbles = !!bubbles;
    this.#cancelable = !!cancelable;
  }

  stopImmediatePropagation() {
    if (!isEvent(this)) throw invalidThis('Event');
    this.#propagationStopped = true;
    this.#stop = true;
  }

  preventDefault() {
    if (!isEvent(this)) throw invalidThis('Event');
    if (this.#cancelable) this.#defaultPrevented = true;
  }

  get target() {
    if (!isEvent(this)) throw invalidThis('Event');
    return this.#target;
  }

  get currentTarget() {
    if (!isEvent(this)) throw invalidThis('Event');
    return this.#dispatching ? this.#target : null;
  }

  get srcElement() {
    if (!isEvent(this)) throw invalidThis('Event');
    return this.#target;
  }

  get type() {
    if (!isEvent(this)) throw invalidThis('Event');
    return this.#type;
  }

  get cancelable() {
    if (!isEvent(this)) throw invalidThis('Event');
    return this.#cancelable;
  }

  get defaultPrevented() {
    if (!isEvent(this)) throw invalidThis('Event');
    return this.#cancelable && this.#defaultPrevented;
  }

  get timeStamp() {
    if (!isEvent(this)) throw invalidThis('Event');
    return this.#timestamp;
  }

  composedPath() {
    if (!isEvent(this)) throw invalidThis('Event');
    return this.#dispatching ? [this.#target] : [];
  }

  get returnValue() {
    if (!isEvent(this)) throw invalidThis('Event');
    return !this.#cancelable || !this.#defaultPrevented;
  }

  set returnValue(value) {
    if (!isEvent(this)) throw invalidThis('Event');
    if (!value && this.#cancelable) this.#defaultPrevented = true;
  }

  get bubbles() {
    if (!isEvent(this)) throw invalidThis('Event');
    return this.#bubbles;
  }

  get composed() {
    if (!isEvent(this)) throw invalidThis('Event');
    return this.#composed;
  }

  get eventPhase() {
    if (!isEvent(this)) throw invalidThis('Event');
    return this.#dispatching ? 2 : 0;
  }

  get cancelBubble() {
    if (!isEvent(this)) throw invalidThis('Event');
    return this.#propagationStopped;
  }

  set cancelBubble(value) {
    if (!isEvent(this)) throw invalidThis('Event');
    if (value) this.#propagationStopped = true;
  }

  stopPropagation() {
    if (!isEvent(this)) throw invalidThis('Event');
    this.#propagationStopped = true;
  }

  get isTrusted() {
    return isEvent(this) && this.#trusted;
  }

  [inspectCustom](depth, options, inspect) {
    const name = this.constructor.name;
    if (depth < 0) return name;
    return `${name} ${inspect({
      type: this.#type,
      defaultPrevented: this.#defaultPrevented,
      cancelable: this.#cancelable,
      timeStamp: this.#timestamp,
    }, options)}`;
  }
}

const isEvent = Event.is;
const setDispatch = Event.setDispatch;
const stopped = Event.stopped;
delete Event.is;
delete Event.setDispatch;
delete Event.stopped;
for (const [k, v] of [['NONE', 0], ['CAPTURING_PHASE', 1], ['AT_TARGET', 2], ['BUBBLING_PHASE', 3]]) {
  Object.defineProperty(Event, k, { value: v, writable: false, enumerable: true, configurable: false });
}

// node's internal/event_target symbols (listener options the streams pass).
const kWeakHandler = Symbol('kWeak');
// Registered: `events.addAbortListener` passes it too.
const kResistStopPropagation = Symbol.for('js2wasi3.resistStopPropagation');

class EventTarget {
  // type → listener records `{ listener, once, capture, passive, removed, resist }`, in order.
  #events = new Map();

  static is(v) {
    return typeof v === 'object' && v !== null && #events in v;
  }

  addEventListener(...args) {
    if (!isEventTarget(this)) throw invalidThis('EventTarget');
    if (args.length < 2) {
      throw codeError(TypeError, 'ERR_MISSING_ARGS', 'The "type" and "listener" arguments must be specified');
    }
    let [type, listener, options] = args;
    if (listener === null || listener === undefined) return;
    if (typeof listener !== 'function' && typeof listener !== 'object') {
      throw codeError(TypeError, 'ERR_INVALID_ARG_TYPE',
        `The "listener" argument must be an instance of EventListener. Received ${$$typeDescribe(listener)}`);
    }
    const opts = typeof options === 'boolean' ? { capture: options } : options || {};
    const signal = opts.signal;
    if (signal !== undefined && signal.aborted) return;
    type = `${type}`;
    const capture = !!opts.capture;
    let list = this.#events.get(type);
    if (list === undefined) {
      list = [];
      this.#events.set(type, list);
    }
    for (const r of list) if (r.listener === listener && r.capture === capture) return;
    list.push({ listener, once: !!opts.once, capture, passive: !!opts.passive, removed: false, resist: !!opts[kResistStopPropagation] });
    if (signal !== undefined) {
      signal.addEventListener('abort', () => this.removeEventListener(type, listener, options), { once: true, [kResistStopPropagation]: true });
    }
  }

  removeEventListener(type, listener, options = undefined) {
    if (!isEventTarget(this)) throw invalidThis('EventTarget');
    const capture = typeof options === 'boolean' ? options : !!(options && options.capture);
    const list = this.#events.get(`${type}`);
    if (list === undefined) return;
    for (let i = 0; i < list.length; i++) {
      if (list[i].listener === listener && list[i].capture === capture) {
        list[i].removed = true;
        list.splice(i, 1);
        return;
      }
    }
  }

  dispatchEvent(...args) {
    if (!isEventTarget(this)) throw invalidThis('EventTarget');
    if (args.length < 1) throw codeError(TypeError, 'ERR_MISSING_ARGS', 'The "event" argument must be specified');
    const event = args[0];
    if (!isEvent(event)) {
      throw codeError(TypeError, 'ERR_INVALID_ARG_TYPE',
        `The "event" argument must be an instance of Event. Received ${$$typeDescribe(event)}`);
    }
    if (event.eventPhase !== 0) throw $$domException('The event is already being dispatched', 'InvalidStateError');
    setDispatch(event, this, true);
    const list = this.#events.get(event.type);
    if (list !== undefined) {
      for (const r of list.slice()) {
        if (r.removed || (stopped(event) && !r.resist)) continue;
        if (r.once) this.removeEventListener(event.type, r.listener, { capture: r.capture });
        const l = r.listener;
        try {
          let result;
          if (typeof l === 'function') result = l.call(this, event);
          else if (l !== null && typeof l.handleEvent === 'function') result = l.handleEvent(event);
          // node: the dispatch flag is cleared after a listener (so later ones see phase 0).
          setDispatch(event, undefined, false);
          if (result !== undefined && result !== null && typeof result.then === 'function') {
            result.then(undefined, (err) => process.nextTick(() => {
              throw err;
            }));
          }
        } catch (err) {
          // node reports a listener's error as uncaught on the next tick.
          process.nextTick(() => {
            throw err;
          });
        }
      }
    }
    setDispatch(event, undefined, false);
    return event.defaultPrevented !== true;
  }

  [inspectCustom](depth, options, inspect) {
    const name = this.constructor.name;
    if (depth < 0) return name;
    return `${name} {}`;
  }
}

const isEventTarget = EventTarget.is;
delete EventTarget.is;

const kCreate = Symbol('kCreate');

function illegal() {
  return codeError(TypeError, 'ERR_ILLEGAL_CONSTRUCTOR', 'Illegal constructor');
}

class AbortSignal extends EventTarget {
  #aborted = false;
  #reason = undefined;
  #onabort = null;
  #onabortListener = null;

  constructor(key) {
    if (key !== kCreate) throw illegal();
    super();
  }

  static is(v) {
    return typeof v === 'object' && v !== null && #aborted in v;
  }

  static set(s, reason) {
    if (s.#aborted) return false;
    s.#aborted = true;
    s.#reason = reason;
    return true;
  }

  get aborted() {
    if (!isAbortSignal(this)) throw invalidThis('AbortSignal');
    return this.#aborted;
  }

  get reason() {
    if (!isAbortSignal(this)) throw invalidThis('AbortSignal');
    return this.#reason;
  }

  throwIfAborted() {
    if (!isAbortSignal(this)) throw invalidThis('AbortSignal');
    if (this.#aborted) throw this.#reason;
  }

  // An event handler attribute: its listener keeps the place where it was first set.
  get onabort() {
    if (!isAbortSignal(this)) throw invalidThis('AbortSignal');
    return this.#onabort;
  }

  set onabort(value) {
    if (!isAbortSignal(this)) throw invalidThis('AbortSignal');
    this.#onabort = typeof value === 'function' || (typeof value === 'object' && value !== null) ? value : null;
    if (this.#onabortListener === null && this.#onabort !== null) {
      const self = this;
      this.#onabortListener = function (event) {
        const h = self.#onabort;
        if (typeof h === 'function') return h.call(self, event);
      };
      this.addEventListener('abort', this.#onabortListener);
    }
  }

  static abort(reason = abortError()) {
    const s = new AbortSignal(kCreate);
    setAborted(s, reason);
    return s;
  }

  static timeout(delay) {
    const s = new AbortSignal(kCreate);
    // node's timer doesn't keep the process alive.
    setTimeout(() => abortSignal(s, $$domException('The operation was aborted due to timeout', 'TimeoutError')), delay).unref();
    return s;
  }

  static any(signals) {
    const s = new AbortSignal(kCreate);
    for (const t of signals) {
      if (t.aborted) {
        setAborted(s, t.reason);
        return s;
      }
    }
    for (const t of signals) t.addEventListener('abort', () => abortSignal(s, t.reason), { once: true });
    return s;
  }

  [inspectCustom](depth, options, inspect) {
    return `AbortSignal ${inspect({ aborted: this.#aborted }, options)}`;
  }
}

const isAbortSignal = AbortSignal.is;
const setAborted = AbortSignal.set;
delete AbortSignal.is;
delete AbortSignal.set;

function abortError() {
  return $$domException('This operation was aborted', 'AbortError');
}

function abortSignal(signal, reason) {
  if (!setAborted(signal, reason)) return;
  signal.dispatchEvent(new Event('abort', { [kTrust]: true }));
}

class AbortController {
  #signal = new AbortSignal(kCreate);

  get signal() {
    if (!(typeof this === 'object' && this !== null && #signal in this)) throw invalidThis('AbortController');
    return this.#signal;
  }

  abort(reason = abortError()) {
    if (!(typeof this === 'object' && this !== null && #signal in this)) throw invalidThis('AbortController');
    abortSignal(this.#signal, reason);
  }

  [inspectCustom](depth, options, inspect) {
    return `AbortController ${inspect({ signal: this.#signal }, options)}`;
  }
}

for (const C of [Event, EventTarget, AbortSignal, AbortController]) {
  Object.defineProperty(C.prototype, Symbol.toStringTag, { value: C.name, writable: false, enumerable: false, configurable: true });
  Object.defineProperty(C.prototype, inspectCustom, { enumerable: false });
}

// node's events.addAbortListener (also for the streams).
const { addAbortListener } = require('node:events');

module.exports = { Event, EventTarget, AbortSignal, AbortController, kWeakHandler, kResistStopPropagation, addAbortListener };
