// node's `util` (T55): `format`, `inspect` (the runtime's formatter: only the `depth` option is
// honoured, and past depth 2 node's `compact: 3` line breaks aren't: F240), `inherits`, `promisify`, `callbackify`, `deprecate`, `debuglog`, `isDeepStrictEqual`,
// `types` (the checks js2wasi3's values can answer), `stripVTControlCharacters`, `toUSVString`,
// `isArray`. Parts follow node 24's lib/util.js and lib/internal/util*.js (MIT, Node.js
// contributors). The rest of node's members throw when called (F240).
"use strict";

const kCustomInspect = Symbol.for("nodejs.util.inspect.custom");
const kCustomPromisify = Symbol.for("nodejs.util.promisify.custom");

function notImplemented(what) {
  return function () {
    throw new Error("util." + what + " isn't implemented in js2wasi3 (planned: F240)");
  };
}

function validateFunction(value, name) {
  if (typeof value !== "function") throw $$errInvalidArgType(name, "of type function", value);
}

function validateObject(value, name) {
  if (value === null || typeof value !== "object") throw $$errInvalidArgType(name, "of type object", value);
}

// `util.inspect(value[, options])`, or the legacy `(value, showHidden, depth, colors)`.
function inspect(value, opts) {
  let depth = 2;
  if (opts !== null && typeof opts === "object") {
    if (opts.depth !== undefined) depth = opts.depth;
  } else if (arguments.length > 2 && arguments[2] !== undefined) {
    depth = arguments[2];
  }
  if (depth === 2) return __intrinsic_inspect(value, 0);
  if (depth === null || depth > 1000000) depth = 1000000;
  return __intrinsic_inspect_depth(value, 2 - Math.floor(depth));
}
inspect.custom = kCustomInspect;
inspect.defaultOptions = {
  showHidden: false,
  depth: 2,
  colors: false,
  customInspect: true,
  showProxy: false,
  maxArrayLength: 100,
  maxStringLength: 10000,
  breakLength: 80,
  compact: 3,
  sorted: false,
  getters: false,
  numericSeparator: false,
};

function format(...args) {
  return $$consoleFormat(args);
}

function formatWithOptions(inspectOptions, ...args) {
  validateObject(inspectOptions, "inspectOptions");
  return $$consoleFormat(args);
}

function inherits(ctor, superCtor) {
  if (ctor === undefined || ctor === null) throw $$errInvalidArgType("ctor", "of type function", ctor);
  if (superCtor === undefined || superCtor === null) throw $$errInvalidArgType("superCtor", "of type function", superCtor);
  if (superCtor.prototype === undefined) {
    throw $$errInvalidArgType("superCtor.prototype", "of type object", superCtor.prototype);
  }
  Object.defineProperty(ctor, "super_", {
    value: superCtor,
    writable: true,
    configurable: true,
  });
  Object.setPrototypeOf(ctor.prototype, superCtor.prototype);
}

// Copies `from`'s own properties (their descriptors) to `to`, as node's
// `ObjectDefineProperties(to, ObjectGetOwnPropertyDescriptors(from))`.
function copyDescriptors(to, from, edit) {
  const keys = Reflect.ownKeys(from);
  for (let i = 0; i < keys.length; i++) {
    const d = Object.getOwnPropertyDescriptor(from, keys[i]);
    if (edit !== undefined) edit(keys[i], d);
    Object.defineProperty(to, keys[i], d);
  }
  return to;
}

function promisify(original) {
  validateFunction(original, "original");
  if (original[kCustomPromisify]) {
    const fn = original[kCustomPromisify];
    validateFunction(fn, "util.promisify.custom");
    return Object.defineProperty(fn, kCustomPromisify, {
      value: fn,
      enumerable: false,
      writable: false,
      configurable: true,
    });
  }
  function fn(...args) {
    return new Promise((resolve, reject) => {
      args.push((err, ...values) => {
        if (err) {
          return reject(err);
        }
        resolve(values[0]);
      });
      Reflect.apply(original, this, args);
    });
  }
  Object.setPrototypeOf(fn, Object.getPrototypeOf(original));
  Object.defineProperty(fn, kCustomPromisify, {
    value: fn,
    enumerable: false,
    writable: false,
    configurable: true,
  });
  return copyDescriptors(fn, original);
}
promisify.custom = kCustomPromisify;

function callbackifyOnRejected(reason, cb) {
  // `!reason` guard inspired by bluebird (Ref: https://goo.gl/t5IS6M). Because `null` is a
  // special error value in callbacks which means "no error occurred", we error-wrap so the
  // callback consumer can distinguish between "the promise rejected with null" or "the promise
  // fulfilled with undefined".
  if (!reason) {
    const newReason = new Error("Promise was rejected with falsy value");
    newReason.code = "ERR_FALSY_VALUE_REJECTION";
    newReason.reason = reason;
    reason = newReason;
  }
  return cb(reason);
}

function callbackify(original) {
  validateFunction(original, "original");
  // We DO NOT return the promise as it gives the user a false sense that the promise is actually
  // somehow related to the callback's execution and that the callback throwing will reject the
  // promise.
  function callbackified(...args) {
    const maybeCb = args.pop();
    validateFunction(maybeCb, "last argument");
    const cb = maybeCb.bind(this);
    // In true node style we process the callback on `nextTick` with all the implications
    // (stack, `uncaughtException`, `async_hooks`)
    Reflect.apply(original, this, args).then(
      (ret) => {
        process.nextTick(() => cb(null, ret));
      },
      (rej) => {
        process.nextTick(() => callbackifyOnRejected(rej, cb));
      },
    );
  }
  return copyDescriptors(callbackified, original, (key, d) => {
    if (key === "length" && typeof d.value === "number") d.value++;
    if (key === "name" && typeof d.value === "string") d.value += "Callbackified";
  });
}

// Mark that a method should not be used: a warning (not printed, as node with `--no-warnings`)
// the first time the returned function is called.
function deprecate(fn, msg, code) {
  validateFunction(fn, "fn");
  let warned = false;
  function deprecated(...args) {
    if (!warned) {
      warned = true;
      if (code !== undefined) {
        process.emitWarning(msg, "DeprecationWarning", code);
      } else {
        process.emitWarning(msg, "DeprecationWarning");
      }
    }
    if (new.target) {
      return Reflect.construct(fn, args, new.target);
    }
    return Reflect.apply(fn, this, args);
  }
  // The wrapper will keep the same prototype as fn to maintain prototype chain.
  Object.setPrototypeOf(deprecated, fn);
  if (fn.prototype) {
    // Setting this (rather than using Object.setPrototype, as above) ensures that calling the
    // unwrapped constructor gives an instanceof the wrapped constructor.
    deprecated.prototype = fn.prototype;
  }
  return deprecated;
}

// `NODE_DEBUG=a,b*` enables `debuglog("a")`; lines go to stderr as `A: message` (node adds the
// process id, which WASI doesn't have).
function debuglog(set, cb) {
  const pattern = process.env.NODE_DEBUG || "";
  const name = set.toUpperCase();
  let enabled = false;
  const parts = pattern.split(",");
  for (let i = 0; i < parts.length; i++) {
    const p = parts[i].trim().toUpperCase();
    if (p === "") continue;
    if (p === name || (p.endsWith("*") && name.startsWith(p.slice(0, -1))) || p === "*") enabled = true;
  }
  const logger = function debug(...args) {
    if (enabled) process.stderr.write(name + ": " + $$consoleFormat(args) + "\n");
  };
  Object.defineProperty(logger, "enabled", {
    get() {
      return enabled;
    },
    configurable: true,
    enumerable: true,
  });
  if (typeof cb === "function") cb(logger);
  return logger;
}

function isArray(v) {
  return Array.isArray(v);
}

// ANSI escape sequences (node's `ansi` pattern).
const ansi = /[\u001B\u009B][[\]()#;?]*(?:(?:(?:(?:;[-a-zA-Z\d\/\#&.:=?%@~_]+)*|[a-zA-Z\d]+(?:;[-a-zA-Z\d\/\#&.:=?%@~_]*)*)?(?:\u0007|\u001B\|\u009C))|(?:(?:\d{1,4}(?:;\d{0,4})*)?[\dA-PR-TZcf-ntqry=><~]))/g;

function stripVTControlCharacters(str) {
  if (typeof str !== "string") throw $$errInvalidArgType("str", "of type string", str);
  return str.replace(ansi, "");
}

function toUSVString(input) {
  return String(input).toWellFormed();
}

// `util.types`: from the runtime's built-in tags (no Proxy, WeakMap/WeakSet,
// SharedArrayBuffer or external values in js2wasi3).
function tagOf(v) {
  return (typeof v === "object" && v !== null) || typeof v === "function" ? __intrinsic_builtin_tag(v) : undefined;
}
const typedNames = [
  "Int8Array", "Uint8Array", "Uint8ClampedArray", "Int16Array", "Uint16Array", "Int32Array", "Uint32Array",
  "Float16Array", "Float32Array", "Float64Array", "BigInt64Array", "BigUint64Array",
];
function isTyped(name) {
  return (v) => tagOf(v) === name;
}
function isTag(name) {
  return (v) => tagOf(v) === name;
}
const no = (v) => false;
const types = {
  isArgumentsObject: (v) => typeof v === "object" && v !== null && Object.prototype.toString.call(v) === "[object Arguments]",
  isArrayBuffer: isTag("ArrayBuffer"),
  isAsyncFunction: (v) => tagOf(v) === "AsyncFunction" || tagOf(v) === "AsyncGeneratorFunction",
  isBigIntObject: isTag("BigInt"),
  isBooleanObject: isTag("Boolean"),
  isDate: (v) => typeof v === "object" && v !== null && __intrinsic_date_data(v) !== undefined,
  isExternal: no,
  isGeneratorFunction: (v) => tagOf(v) === "GeneratorFunction" || tagOf(v) === "AsyncGeneratorFunction",
  isGeneratorObject: (v) => tagOf(v) === "Generator" || tagOf(v) === "AsyncGenerator",
  isMap: isTag("Map"),
  isMapIterator: isTag("Map Iterator"),
  isModuleNamespaceObject: isTag("Module"),
  isNativeError: (v) => v instanceof Error,
  isNumberObject: isTag("Number"),
  isPromise: isTag("Promise"),
  isProxy: no,
  isRegExp: (v) => typeof v === "object" && v !== null && !!__intrinsic_regexp_data(v),
  isSet: isTag("Set"),
  isSetIterator: isTag("Set Iterator"),
  isSharedArrayBuffer: no,
  isStringObject: isTag("String"),
  isSymbolObject: isTag("Symbol"),
  isWeakMap: no,
  isWeakSet: no,
  isAnyArrayBuffer: isTag("ArrayBuffer"),
  isBoxedPrimitive: (v) => {
    const t = tagOf(v);
    return t === "Number" || t === "String" || t === "Boolean" || t === "BigInt" || t === "Symbol";
  },
  isArrayBufferView: (v) => typedNames.includes(tagOf(v)) || tagOf(v) === "DataView",
  isDataView: isTag("DataView"),
  isTypedArray: (v) => typedNames.includes(tagOf(v)),
  isUint8Array: isTyped("Uint8Array"),
  isUint8ClampedArray: isTyped("Uint8ClampedArray"),
  isUint16Array: isTyped("Uint16Array"),
  isUint32Array: isTyped("Uint32Array"),
  isInt8Array: isTyped("Int8Array"),
  isInt16Array: isTyped("Int16Array"),
  isInt32Array: isTyped("Int32Array"),
  isFloat16Array: isTyped("Float16Array"),
  isFloat32Array: isTyped("Float32Array"),
  isFloat64Array: isTyped("Float64Array"),
  isBigInt64Array: isTyped("BigInt64Array"),
  isBigUint64Array: isTyped("BigUint64Array"),
  isKeyObject: no,
  isCryptoKey: no,
};
// node's checks have no names.
{
  const keys = Object.keys(types);
  for (let i = 0; i < keys.length; i++) {
    Object.defineProperty(types[keys[i]], "name", { value: "" });
  }
}

// `isDeepStrictEqual`: node's strict deep equality (lib/internal/util/comparisons.js), for the
// values js2wasi3 has.
function ownKeys(o) {
  const keys = Object.keys(o);
  const symbols = Object.getOwnPropertySymbols(o);
  for (let i = 0; i < symbols.length; i++) {
    if (Object.prototype.propertyIsEnumerable.call(o, symbols[i])) keys.push(symbols[i]);
  }
  return keys;
}

function deepEqual(a, b, memo) {
  if (a === b) return a !== 0 || Object.is(a, b);
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) {
    return typeof a === "number" && typeof b === "number" && a !== a && b !== b;
  }
  if (Object.getPrototypeOf(a) !== Object.getPrototypeOf(b)) return false;
  const ta = Object.prototype.toString.call(a);
  if (ta !== Object.prototype.toString.call(b)) return false;
  for (let i = 0; i < memo.length; i++) {
    if (memo[i][0] === a && memo[i][1] === b) return true;
  }
  memo.push([a, b]);
  try {
    return deepEqualObjects(a, b, ta, memo);
  } finally {
    memo.pop();
  }
}

function deepEqualObjects(a, b, tag, memo) {
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) return false;
  } else if (tag === "[object Date]") {
    if (a.getTime() !== b.getTime()) return false;
  } else if (tag === "[object RegExp]") {
    if (a.source !== b.source || a.flags !== b.flags || a.lastIndex !== b.lastIndex) return false;
  } else if (a instanceof Error) {
    if (a.message !== b.message || a.name !== b.name) return false;
  }
  const t = tagOf(a);
  if (t !== tagOf(b)) return false;
  if (t === "Number" || t === "String" || t === "Boolean" || t === "BigInt" || t === "Symbol") {
    if (!Object.is(a.valueOf(), b.valueOf())) return false;
  }
  if (typedNames.includes(t)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
      if (!Object.is(a[i], b[i])) return false;
    }
  }
  const ka = ownKeys(a);
  const kb = ownKeys(b);
  if (ka.length !== kb.length) return false;
  for (let i = 0; i < ka.length; i++) {
    if (!Object.prototype.propertyIsEnumerable.call(b, ka[i])) return false;
  }
  for (let i = 0; i < ka.length; i++) {
    if (!deepEqual(a[ka[i]], b[ka[i]], memo)) return false;
  }
  if (t === "Set") {
    if (a.size !== b.size) return false;
    for (const v of a) {
      if (b.has(v)) continue;
      if (typeof v !== "object" || v === null) return false;
      let found = false;
      for (const w of b) {
        if (typeof w === "object" && w !== null && !a.has(w) && deepEqual(v, w, memo)) {
          found = true;
          break;
        }
      }
      if (!found) return false;
    }
  } else if (t === "Map") {
    if (a.size !== b.size) return false;
    for (const [k, v] of a) {
      if (b.has(k)) {
        if (!deepEqual(v, b.get(k), memo)) return false;
        continue;
      }
      if (typeof k !== "object" || k === null) return false;
      let found = false;
      for (const [k2, v2] of b) {
        if (typeof k2 === "object" && k2 !== null && !a.has(k2) && deepEqual(k, k2, memo) && deepEqual(v, v2, memo)) {
          found = true;
          break;
        }
      }
      if (!found) return false;
    }
  }
  return true;
}

function isDeepStrictEqual(a, b) {
  return deepEqual(a, b, []);
}

function getSystemErrorName(err) {
  if (typeof err !== "number") throw $$errInvalidArgType("err", "of type number", err);
  if (err >= 0 || !Number.isSafeInteger(err)) {
    throw $$errOutOfRange("err", "a negative integer", err);
  }
  return "Unknown system error " + err;
}

module.exports = {
  _errnoException: notImplemented("_errnoException"),
  _exceptionWithHostPort: notImplemented("_exceptionWithHostPort"),
  _extend: function _extend(target, source) {
    // Don't do anything if source isn't an object
    if (source === null || typeof source !== "object") return target;
    const keys = Object.keys(source);
    let i = keys.length;
    while (i--) {
      target[keys[i]] = source[keys[i]];
    }
    return target;
  },
  callbackify,
  convertProcessSignalToExitCode: notImplemented("convertProcessSignalToExitCode"),
  debug: debuglog,
  debuglog,
  deprecate,
  format,
  styleText: notImplemented("styleText"),
  formatWithOptions,
  getCallSites: notImplemented("getCallSites"),
  getSystemErrorMap: notImplemented("getSystemErrorMap"),
  getSystemErrorName,
  getSystemErrorMessage: notImplemented("getSystemErrorMessage"),
  inherits,
  inspect,
  isArray: deprecate(isArray, "The `util.isArray` API is deprecated. Please use `Array.isArray()` instead.", "DEP0044"),
  isDeepStrictEqual,
  promisify,
  stripVTControlCharacters,
  toUSVString,
  transferableAbortSignal: notImplemented("transferableAbortSignal"),
  transferableAbortController: notImplemented("transferableAbortController"),
  aborted: notImplemented("aborted"),
  types,
  parseEnv: notImplemented("parseEnv"),
  parseArgs: notImplemented("parseArgs"),
  TextDecoder: notImplemented("TextDecoder"),
  TextEncoder: notImplemented("TextEncoder"),
  MIMEType: notImplemented("MIMEType"),
  MIMEParams: notImplemented("MIMEParams"),
  diff: notImplemented("diff"),
  setTraceSigInt: notImplemented("setTraceSigInt"),
};
