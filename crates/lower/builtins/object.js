// Objects (T21 slice 4). Conventions: crates/lower/src/builtins.rs.
// `$$Object_x` is `Object.x`, `$$Object_prototype_x` is `Object.prototype.x`. Object intrinsics:
// `define(o, k, v)` (CreateDataProperty on an object). Not implemented: wrapper objects for primitive `this` / targets (they trap).
// Uses string.js's `$$String`.

// ToObject for an object or a primitive whose properties are only read.
function $$toObjectCheck(v) {
  if (v === undefined || v === null) {
    throw new TypeError("Cannot convert undefined or null to object");
  }
  return v;
}

// ---- Object ------------------------------------------------------------------------------

// `Object(v)` / `new Object(v)` (T25): a new object for `undefined`/`null`, objects unchanged.
function $$Object(v) {
  if (v === undefined || v === null) {
    return {};
  }
  const t = typeof v;
  if (t !== "object" && t !== "function") {
    // A wrapper object (F88).
    __intrinsic_trap();
  }
  return v;
}

// `Boolean(v)` (T25; `new Boolean` wrapper objects: F88).
function $$Boolean(v) {
  return !!v;
}

function $$Object_assign(target, ...sources) {
  const to = $$toObjectCheck(target);
  const t = typeof to;
  if (t !== "object" && t !== "function") {
    // A wrapper object (F88).
    __intrinsic_trap();
  }
  const n = +sources.length;
  for (let i = 0; i < n; i++) {
    const from = sources[i];
    if (from === undefined || from === null) {
      continue;
    }
    if (typeof from === "string") {
      const len = __intrinsic_str_len(from);
      for (let k = 0; k < len; k++) {
        to[k] = __intrinsic_sub(from, k, k + 1);
      }
      continue;
    }
    const keys = Object.keys(from);
    const m = +keys.length;
    for (let k = 0; k < m; k++) {
      const key = keys[k];
      to[key] = from[key];
    }
    // Then the enumerable symbol keys (T25).
    if (typeof from === "object" || typeof from === "function") {
      const syms = Object.getOwnPropertySymbols(from);
      const ns = +syms.length;
      for (let k = 0; k < ns; k++) {
        const key = syms[k];
        if (Object.getOwnPropertyDescriptor(from, key).enumerable) {
          to[key] = from[key];
        }
      }
    }
  }
  return to;
}

function $$Object_fromEntries(iterable) {
  if (iterable === undefined || iterable === null) {
    throw new TypeError("undefined is not iterable");
  }
  const obj = {};
  for (const entry of iterable) {
    const t = typeof entry;
    if ((t !== "object" && t !== "function") || entry === null) {
      throw new TypeError("Iterator value " + $$String(entry) + " is not an entry object");
    }
    __intrinsic_define(obj, entry[0], entry[1]);
  }
  return obj;
}

// `Object.groupBy(items, callbackfn)` (node 21+): a null-prototype object of arrays, in
// first-seen key order.
function $$Object_groupBy(items, callbackfn) {
  if (items === undefined || items === null) {
    throw new TypeError("Object.groupBy called on null or undefined");
  }
  if (typeof callbackfn !== "function") {
    throw new TypeError($$describe(callbackfn) + " is not a function");
  }
  $$mapIterable(items);
  const o = Object.create(null);
  let k = 0;
  for (const v of items) {
    let key = callbackfn(v, k);
    if (typeof key !== "symbol") {
      key = $$String(key);
    }
    let group = o[key];
    if (group === undefined) {
      group = [];
      __intrinsic_define(o, key, group);
    }
    group.push(v);
    k++;
  }
  return o;
}

// ---- Object.prototype --------------------------------------------------------------------

// The legacy accessor methods (Annex B).
function $$Object_prototype___defineGetter__(p, getter) {
  const o = $$toObjectCheck(this);
  if (typeof getter !== "function") {
    throw new TypeError("Object.prototype.__defineGetter__: Expecting function");
  }
  Object.defineProperty(o, p, { get: getter, enumerable: true, configurable: true });
}

function $$Object_prototype___defineSetter__(p, setter) {
  const o = $$toObjectCheck(this);
  if (typeof setter !== "function") {
    throw new TypeError("Object.prototype.__defineSetter__: Expecting function");
  }
  Object.defineProperty(o, p, { set: setter, enumerable: true, configurable: true });
}

function $$lookupAccessor(o, p, setter) {
  for (;;) {
    const d = Object.getOwnPropertyDescriptor(o, p);
    if (d !== undefined) {
      return setter ? d.set : d.get;
    }
    o = Object.getPrototypeOf(o);
    if (o === null) {
      return undefined;
    }
  }
  return undefined;
}

function $$Object_prototype___lookupGetter__(p) {
  return $$lookupAccessor($$toObjectCheck(this), p, false);
}

function $$Object_prototype___lookupSetter__(p) {
  return $$lookupAccessor($$toObjectCheck(this), p, true);
}

function $$Object_prototype_hasOwnProperty(v) {
  $$toObjectCheck(this);
  return Object.hasOwn(this, v);
}

function $$Object_prototype_isPrototypeOf(v) {
  const t = typeof v;
  if ((t !== "object" && t !== "function") || v === null) {
    return false;
  }
  const o = $$toObjectCheck(this);
  for (;;) {
    v = Object.getPrototypeOf(v);
    if (v === null) {
      return false;
    }
    if (v === o) {
      return true;
    }
  }
  return false;
}

function $$Object_prototype_propertyIsEnumerable(v) {
  const o = $$toObjectCheck(this);
  if (typeof o === "string") {
    // Its indices are enumerable, `length` isn't.
    return Object.hasOwn(o, v) && $$String(v) !== "length";
  }
  const d = Object.getOwnPropertyDescriptor(o, v);
  return d !== undefined && d.enumerable;
}

function $$Object_prototype_toString() {
  const o = this;
  if (o === undefined) {
    return "[object Undefined]";
  }
  if (o === null) {
    return "[object Null]";
  }
  const t = typeof o;
  if (t === "function") {
    const ftag = __intrinsic_builtin_tag(o);
    return ftag !== undefined ? "[object " + ftag + "]" : "[object Function]";
  }
  if (t === "number") {
    return "[object Number]";
  }
  if (t === "string") {
    return "[object String]";
  }
  if (t === "boolean") {
    return "[object Boolean]";
  }
  if (t === "symbol") {
    return "[object Symbol]";
  }
  if (t === "bigint") {
    return "[object BigInt]";
  }
  // `Symbol.toStringTag` (T25): the object's own when the code names it, else the built-in one.
  const tag = __intrinsic_builtin_tag(o);
  if (tag !== undefined) {
    return "[object " + tag + "]";
  }
  if (Array.isArray(o)) {
    return "[object Array]";
  }
  // `[[ErrorData]]`, approximated by the prototype chain.
  if (o instanceof Error) {
    return "[object Error]";
  }
  return "[object Object]";
}

function $$Object_prototype_toLocaleString() {
  if (this === undefined || this === null) {
    throw new TypeError("Object.prototype.toLocaleString called on null or undefined");
  }
  return this.toString();
}

function $$Object_prototype_valueOf() {
  const o = $$toObjectCheck(this);
  const t = typeof o;
  if (t !== "object" && t !== "function") {
    // A wrapper object (F88).
    __intrinsic_trap();
  }
  return o;
}
