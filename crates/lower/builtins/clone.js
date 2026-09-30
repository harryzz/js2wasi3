// `structuredClone(value)` (T53): the HTML structured clone of V8's serializer, in one pass (no
// bytes in between): primitives, plain objects (own enumerable string keys, getters run), arrays
// (holes and extra properties kept), `Date`, `RegExp` (`lastIndex` 0), `Map`, `Set`, wrapper
// objects, errors (`name` picks the constructor; own `message` and `cause`; `stack` isn't implemented), `ArrayBuffer`s
// and typed arrays (buffers shared by views stay shared). Cycles and shared references are kept
// (a native map from each object to its clone). Anything else throws node's `DataCloneError`, a
// `DOMException` (only as far as this needs: its prototype, `name`, `code`).
// Uses string.js's `$$String`. Not implemented: the `transfer` option, resizable buffers keeping
// their `maxByteLength`, `DOMException` as a global.

let $$domExceptionProto = undefined;

// `DOMException` as the errors' `constructor` (not a global here): not constructible yet.
function $$DOMException() {
  throw new TypeError("DOMException isn't implemented (js2wasi3)");
}

// node's `DOMException [DataCloneError]: <what> could not be cloned.`
function $$dataCloneError(what) {
  if ($$domExceptionProto === undefined) {
    const p = Object.create(Error.prototype);
    Object.defineProperty(p, "constructor", { value: $$DOMException, writable: true, enumerable: false, configurable: true });
    Object.defineProperty(p, "code", { value: 25, writable: false, enumerable: false, configurable: true });
    $$domExceptionProto = p;
  }
  const e = new Error(what + " could not be cloned.");
  Object.setPrototypeOf(e, $$domExceptionProto);
  Object.defineProperty(e, "name", { value: "DataCloneError", writable: true, enumerable: false, configurable: true });
  return e;
}

function $$structuredClone(...args) {
  if (+args.length === 0) {
    const e = new TypeError('The "The value argument must be specified" argument must be specified');
    e.code = "ERR_MISSING_ARGS";
    throw e;
  }
  const options = args[1];
  if (options !== undefined && options !== null) {
    if (typeof options !== "object" && typeof options !== "function") {
      const e = new TypeError("Failed to execute 'structuredClone': Options cannot be converted to a dictionary");
      e.code = "ERR_INVALID_ARG_TYPE";
      throw e;
    }
    const transfer = options.transfer;
    if (transfer !== undefined && !(Array.isArray(transfer) && +transfer.length === 0)) {
      throw new TypeError("structuredClone's transfer option isn't implemented (js2wasi3)");
    }
  }
  return $$cloneValue(args[0], __intrinsic_map_new_map());
}

// An own data property of `o` (the error fields V8 reads without getters).
function $$cloneOwn(o, key) {
  const d = Object.getOwnPropertyDescriptor(o, key);
  return d !== undefined && "value" in d ? d : undefined;
}

function $$cloneValue(v, memo) {
  const t = typeof v;
  if (t === "symbol") {
    throw $$dataCloneError($$String(v));
  }
  if (t === "function") {
    throw $$dataCloneError($$String(v));
  }
  if (t !== "object" || v === null) {
    return v;
  }
  if (__intrinsic_map_has(memo, v)) {
    return __intrinsic_map_get(memo, v);
  }
  if (Array.isArray(v)) {
    const len = +v.length;
    const r = new Array(len);
    __intrinsic_map_set(memo, v, r);
    const keys = Object.keys(v);
    for (let i = 0; i < +keys.length; i++) {
      const k = keys[i];
      __intrinsic_define(r, k, $$cloneValue(v[k], memo));
    }
    return r;
  }
  const date = __intrinsic_date_data(v);
  if (date !== undefined) {
    const r = __intrinsic_date_new([date[0]]);
    __intrinsic_map_set(memo, v, r);
    return r;
  }
  const re = __intrinsic_regexp_data(v);
  if (re) {
    const r = __intrinsic_regexp_new([re[0], re[1], re[2]]);
    __intrinsic_map_set(memo, v, r);
    return r;
  }
  const mapKind = __intrinsic_map_kind(v);
  if (mapKind === 0 || mapKind === 1) {
    const r = mapKind === 0 ? __intrinsic_map_new_map() : __intrinsic_map_new_set();
    __intrinsic_map_set(memo, v, r);
    // The entries first (V8 serializes them all before cloning any).
    const entries = [];
    const it = __intrinsic_map_iter_entries(v);
    for (;;) {
      const i = __intrinsic_map_step(it);
      if (i < 0) {
        break;
      }
      entries.push(__intrinsic_map_key(it, i));
      entries.push(__intrinsic_map_value(it, i));
    }
    for (let i = 0; i < +entries.length; i += 2) {
      const k = $$cloneValue(entries[i], memo);
      __intrinsic_map_set(r, k, mapKind === 0 ? $$cloneValue(entries[i + 1], memo) : undefined);
    }
    return r;
  }
  if (__intrinsic_is_array_buffer(v)) {
    const r = __intrinsic_buffer_slice(v, 0, __intrinsic_buffer_length(v));
    __intrinsic_map_set(memo, v, r);
    return r;
  }
  const kind = __intrinsic_typed_kind(v);
  if (kind >= 0) {
    const buffer = $$cloneValue(__intrinsic_typed_buffer(v), memo);
    const r = __intrinsic_typed_view(kind, buffer, __intrinsic_typed_byte_offset(v), __intrinsic_typed_length(v));
    __intrinsic_map_set(memo, v, r);
    return r;
  }
  const tag = __intrinsic_builtin_tag(v);
  if (tag === "Number" || tag === "String" || tag === "Boolean" || tag === "BigInt") {
    const r = Object(__intrinsic_unwrap(v));
    __intrinsic_map_set(memo, v, r);
    return r;
  }
  if (v instanceof Error) {
    const name = v.name;
    let r;
    const message = $$cloneOwn(v, "message");
    const text = message === undefined ? undefined : $$String(message.value);
    if (name === "EvalError") {
      r = new EvalError(text);
    } else if (name === "RangeError") {
      r = new RangeError(text);
    } else if (name === "ReferenceError") {
      r = new ReferenceError(text);
    } else if (name === "SyntaxError") {
      r = new SyntaxError(text);
    } else if (name === "TypeError") {
      r = new TypeError(text);
    } else if (name === "URIError") {
      r = new URIError(text);
    } else {
      r = new Error(text);
    }
    __intrinsic_map_set(memo, v, r);
    const cause = $$cloneOwn(v, "cause");
    if (cause !== undefined) {
      const c = $$cloneValue(cause.value, memo);
      Object.defineProperty(r, "cause", { value: c, writable: true, enumerable: false, configurable: true });
    }
    return r;
  }
  if (tag !== undefined && tag !== "Math" && tag !== "JSON" && tag !== "Reflect") {
    throw $$dataCloneError(tag === "Promise" ? "#<Promise>" : "[object " + tag + "]");
  }
  if (__intrinsic_is_promise(v)) {
    throw $$dataCloneError("#<Promise>");
  }
  const r = {};
  __intrinsic_map_set(memo, v, r);
  const keys = Object.keys(v);
  for (let i = 0; i < +keys.length; i++) {
    const k = keys[i];
    __intrinsic_define(r, k, $$cloneValue(v[k], memo));
  }
  return r;
}
