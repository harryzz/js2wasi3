// Objects (T21 slice 4). Conventions: crates/lower/src/builtins.rs.
// `$$Object_x` is `Object.x`, `$$Object_prototype_x` is `Object.prototype.x`. Object intrinsics:
// `define(o, k, v)` (CreateDataProperty on an object). Not implemented: symbol keys (the key lists
// come from `Object.keys`), wrapper objects for primitive `this` / targets (they trap).
// Uses string.js's `$$String`.

// ToObject for an object or a primitive whose properties are only read.
function $$toObjectCheck(v) {
  if (v === undefined || v === null) {
    throw new TypeError("Cannot convert undefined or null to object");
  }
  return v;
}

// ---- Object ------------------------------------------------------------------------------

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

// ---- Object.prototype --------------------------------------------------------------------

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
    return "[object Function]";
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
  if (Array.isArray(o)) {
    return "[object Array]";
  }
  // `[[ErrorData]]`, approximated by the prototype chain. `Symbol.toStringTag` isn't supported
  // (`[object Promise]`, `[object Generator]`: F53).
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
