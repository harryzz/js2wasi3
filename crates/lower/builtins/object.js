// Objects (T21 slice 4). Conventions: crates/lower/src/builtins.rs.
// `$$Object_x` is `Object.x`, `$$Object_prototype_x` is `Object.prototype.x`. Object intrinsics:
// `define(o, k, v)` (CreateDataProperty on an object), `to_object(v)` (ToObject: primitives wrapped,
// T30), `unwrap(v)` (a wrapper's primitive).
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
  return __intrinsic_to_object(v);
}

// `Boolean(v)` (T25; `new Boolean(v)` wraps its result, T30).
function $$Boolean(v) {
  return !!v;
}

// thisBooleanValue (T30).
function $$thisBoolean(v, name) {
  v = __intrinsic_unwrap(v);
  if (typeof v !== "boolean") {
    throw new TypeError("Boolean.prototype." + name + " requires that 'this' be a Boolean");
  }
  return v;
}

function $$Boolean_prototype_valueOf() {
  return $$thisBoolean(this, "valueOf");
}

function $$Boolean_prototype_toString() {
  return $$thisBoolean(this, "toString") ? "true" : "false";
}

// `Object.is`: SameValue (T40: BigInts compare by value through `===`).
function $$Object_is(value1, value2) {
  if (typeof value1 === "number" && typeof value2 === "number") {
    if (value1 !== value1) {
      return value2 !== value2;
    }
    if (value1 === 0 && value2 === 0) {
      return 1 / value1 === 1 / value2;
    }
  }
  return value1 === value2;
}

function $$Object_assign(target, ...sources) {
  const to = __intrinsic_to_object(target);
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

// Uses reflect.js's `$$setPrototypeOf` (T37).
function $$Object_setPrototypeOf(o, proto) {
  if (o === undefined || o === null) {
    throw new TypeError("Object.setPrototypeOf called on null or undefined");
  }
  $$checkProto(proto);
  if (!$$isObject(o)) {
    return o;
  }
  const status = $$setPrototypeOf(o, proto);
  if (status === 1) {
    throw new TypeError($$reflectWhat(o) + " is not extensible");
  }
  if (status === 2) {
    throw new TypeError("Cyclic __proto__ value");
  }
  if (status === 3) {
    throw new TypeError("Immutable prototype object 'Object.prototype' cannot have their prototype set");
  }
  return o;
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
    $$append(group, v);
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
  // `[[RegExpMatcher]]` (T27).
  if (__intrinsic_regexp_data(o)) {
    return "[object RegExp]";
  }
  // `[[DateValue]]` (T26).
  if (__intrinsic_date_data(o) !== undefined) {
    return "[object Date]";
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

// ---- Statics lowered to intrinsics, as values (T28) ---------------------------------------
// `const f = Object.defineProperty`: `$$A_b$value` is `A.b` read as a value (calls stay
// intrinsics). Named and sized like node's (`builtins::length`).

function $$Object_defineProperty$value(o, key, desc) {
  return Object.defineProperty(o, key, desc);
}
function $$Object_getOwnPropertyDescriptor$value(o, key) {
  return Object.getOwnPropertyDescriptor(o, key);
}
function $$Object_getOwnPropertyNames$value(o) {
  return Object.getOwnPropertyNames(o);
}
function $$Object_getOwnPropertySymbols$value(o) {
  return Object.getOwnPropertySymbols(o);
}
function $$Object_getPrototypeOf$value(o) {
  return Object.getPrototypeOf(o);
}
function $$Object_hasOwn$value(o, key) {
  return Object.hasOwn(o, key);
}
function $$Object_keys$value(o) {
  return Object.keys(o);
}
function $$Object_values$value(o) {
  return Object.values(o);
}
function $$Object_entries$value(o) {
  return Object.entries(o);
}
function $$Object_preventExtensions$value(o) {
  return Object.preventExtensions(o);
}
function $$Object_seal$value(o) {
  return Object.seal(o);
}
function $$Object_freeze$value(o) {
  return Object.freeze(o);
}
function $$Object_isExtensible$value(o) {
  return Object.isExtensible(o);
}
function $$Object_isSealed$value(o) {
  return Object.isSealed(o);
}
function $$Object_isFrozen$value(o) {
  return Object.isFrozen(o);
}
function $$Array_isArray$value(v) {
  return Array.isArray(v);
}
