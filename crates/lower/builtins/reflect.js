// `Reflect` (T36; no `Proxy`, so every target is an ordinary object or an array/wrapper/typed
// array). Conventions: crates/lower/src/builtins.rs. `$$Reflect_x` is `Reflect.x`. Intrinsics:
// `to_key(v)` (ToPropertyKey), `call_list(f, this, args)` / `construct_list(f, args, newTarget)`
// (the callee already checked), `set_proto(o, p)` (no checks), `object_desc(o)` (`#<Foo>`).
// The writers that return `false` where `Object.*` throws run the user's code (key and descriptor
// conversions, setters) outside the `try` that turns a rejection into `false`.

function $$isObject(v) {
  return (typeof v === "object" && v !== null) || typeof v === "function";
}

function $$reflectTarget(v, name) {
  if (!$$isObject(v)) {
    throw new TypeError("Reflect." + name + " called on non-object");
  }
}

// A value as node shows it in these TypeErrors.
function $$reflectWhat(v) {
  if (typeof v === "string") {
    return v;
  }
  if ($$isObject(v)) {
    return __intrinsic_object_desc(v);
  }
  return String(v);
}

// ToPropertyDescriptor into a plain object with only the fields present (errors as node's).
function $$toPropertyDescriptor(obj) {
  if (!$$isObject(obj)) {
    throw new TypeError("Property description must be an object: " + $$reflectWhat(obj));
  }
  const d = {};
  if ("enumerable" in obj) {
    d.enumerable = !!obj.enumerable;
  }
  if ("configurable" in obj) {
    d.configurable = !!obj.configurable;
  }
  if ("value" in obj) {
    d.value = obj.value;
  }
  if ("writable" in obj) {
    d.writable = !!obj.writable;
  }
  if ("get" in obj) {
    const g = obj.get;
    if (g !== undefined && typeof g !== "function") {
      throw new TypeError("Getter must be a function: " + $$reflectWhat(g));
    }
    d.get = g;
  }
  if ("set" in obj) {
    const s = obj.set;
    if (s !== undefined && typeof s !== "function") {
      throw new TypeError("Setter must be a function: " + $$reflectWhat(s));
    }
    d.set = s;
  }
  if (("get" in d || "set" in d) && ("value" in d || "writable" in d)) {
    throw new TypeError(
      "Invalid property descriptor. Cannot both specify accessors and a value or writable attribute, " +
        $$reflectWhat(obj),
    );
  }
  return d;
}

// [[DefineOwnProperty]] of a converted key and descriptor: `false` when rejected. An array's
// `length` is converted here (ArraySetLength: ToUint32, then ToNumber) so no user code runs in
// the `try`.
function $$reflectDefine(o, key, d) {
  if (key === "length" && "value" in d && Array.isArray(o)) {
    const n = d.value >>> 0;
    if (n !== +d.value) {
      throw new RangeError("Invalid array length");
    }
    d.value = n;
  }
  try {
    Object.defineProperty(o, key, d);
    return true;
  } catch {
    return false;
  }
}

function $$Reflect_apply(target, thisArgument, argumentsList) {
  if (typeof target !== "function") {
    const what =
      target === null ? "null" : typeof target === "object" ? "an object" : "a " + typeof target;
    throw new TypeError(
      "Function.prototype.apply was called on " + $$reflectWhat(target) + ", which is " + what +
        " and not a function",
    );
  }
  if (!$$isObject(argumentsList)) {
    throw new TypeError("CreateListFromArrayLike called on non-object");
  }
  return __intrinsic_call_list(target, thisArgument, argumentsList);
}

function $$Reflect_construct(target, argumentsList, ...rest) {
  if (!__intrinsic_is_constructor(target)) {
    throw new TypeError($$reflectWhat(target) + " is not a constructor");
  }
  const newTarget = +rest.length === 0 ? target : rest[0];
  if (!__intrinsic_is_constructor(newTarget)) {
    throw new TypeError($$reflectWhat(newTarget) + " is not a constructor");
  }
  if (!$$isObject(argumentsList)) {
    throw new TypeError("CreateListFromArrayLike called on non-object");
  }
  return __intrinsic_construct_list(target, argumentsList, newTarget);
}

function $$Reflect_defineProperty(target, propertyKey, attributes) {
  $$reflectTarget(target, "defineProperty");
  const key = __intrinsic_to_key(propertyKey);
  return $$reflectDefine(target, key, $$toPropertyDescriptor(attributes));
}

function $$Reflect_deleteProperty(target, propertyKey) {
  $$reflectTarget(target, "deleteProperty");
  const key = __intrinsic_to_key(propertyKey);
  try {
    return delete target[key];
  } catch {
    return false;
  }
}

// A typed array's element key (CanonicalNumericIndexString): its elements aren't seen by
// `Object.getOwnPropertyDescriptor`, and [[Get]]/[[Set]] on them don't depend on the receiver.
function $$typedElementKey(o, key) {
  return (
    __intrinsic_typed_kind(o) >= 0 && typeof key === "string" && (key === "-0" || String(+key) === key)
  );
}

// OrdinaryGet with a receiver: the getter found on the chain is called with it.
function $$Reflect_get(target, propertyKey, ...rest) {
  $$reflectTarget(target, "get");
  const key = __intrinsic_to_key(propertyKey);
  if (+rest.length === 0 || rest[0] === target || $$typedElementKey(target, key)) {
    return target[key];
  }
  const receiver = rest[0];
  let o = target;
  for (;;) {
    const d = Object.getOwnPropertyDescriptor(o, key);
    if (d !== undefined) {
      if ("value" in d) {
        return d.value;
      }
      const g = d.get;
      return g === undefined ? undefined : __intrinsic_call_list(g, receiver, []);
    }
    o = Object.getPrototypeOf(o);
    if (o === null) {
      return undefined;
    }
  }
}

function $$Reflect_getOwnPropertyDescriptor(target, propertyKey) {
  $$reflectTarget(target, "getOwnPropertyDescriptor");
  return Object.getOwnPropertyDescriptor(target, propertyKey);
}

function $$Reflect_getPrototypeOf(target) {
  $$reflectTarget(target, "getPrototypeOf");
  return Object.getPrototypeOf(target);
}

function $$Reflect_has(target, propertyKey) {
  $$reflectTarget(target, "has");
  return propertyKey in target;
}

function $$Reflect_isExtensible(target) {
  $$reflectTarget(target, "isExtensible");
  return Object.isExtensible(target);
}

function $$Reflect_ownKeys(target) {
  $$reflectTarget(target, "ownKeys");
  const keys = Object.getOwnPropertyNames(target);
  const symbols = Object.getOwnPropertySymbols(target);
  const n = +symbols.length;
  for (let i = 0; i < n; i++) {
    keys.push(symbols[i]);
  }
  return keys;
}

function $$Reflect_preventExtensions(target) {
  $$reflectTarget(target, "preventExtensions");
  Object.preventExtensions(target);
  return true;
}

// OrdinarySet with a receiver (OrdinarySetWithOwnDescriptor): `false` instead of a TypeError.
function $$Reflect_set(target, propertyKey, V, ...rest) {
  $$reflectTarget(target, "set");
  const key = __intrinsic_to_key(propertyKey);
  const receiver = +rest.length === 0 ? target : rest[0];
  if (receiver === target && $$typedElementKey(target, key)) {
    // A number key: `ta["0"] = v` makes a dictionary property today (F163).
    if (key !== "-0") {
      target[+key] = V;
    }
    return true;
  }
  let o = target;
  let own;
  for (;;) {
    own = Object.getOwnPropertyDescriptor(o, key);
    if (own !== undefined) {
      break;
    }
    o = Object.getPrototypeOf(o);
    if (o === null) {
      own = { value: undefined, writable: true };
      break;
    }
  }
  if ("value" in own) {
    if (!own.writable || !$$isObject(receiver)) {
      return false;
    }
    const existing = Object.getOwnPropertyDescriptor(receiver, key);
    if (existing !== undefined) {
      if (!("value" in existing) || !existing.writable) {
        return false;
      }
      return $$reflectDefine(receiver, key, { value: V });
    }
    return $$reflectDefine(receiver, key, {
      value: V,
      writable: true,
      enumerable: true,
      configurable: true,
    });
  }
  const setter = own.set;
  if (setter === undefined) {
    return false;
  }
  __intrinsic_call_list(setter, receiver, [V]);
  return true;
}

// [[SetPrototypeOf]] of an ordinary object after the argument checks: 0 done, 1 not extensible,
// 2 a cycle, 3 `Object.prototype` (an immutable prototype exotic object). Also `Object.setPrototypeOf`.
function $$setPrototypeOf(target, proto) {
  if (Object.getPrototypeOf(target) === proto) {
    return 0;
  }
  if (target === Object.prototype) {
    return 3;
  }
  if (!Object.isExtensible(target)) {
    return 1;
  }
  for (let p = proto; p !== null; p = Object.getPrototypeOf(p)) {
    if (p === target) {
      return 2;
    }
  }
  __intrinsic_set_proto(target, proto);
  return 0;
}

function $$checkProto(proto) {
  if (proto !== null && !$$isObject(proto)) {
    throw new TypeError("Object prototype may only be an Object or null: " + $$reflectWhat(proto));
  }
}

function $$Reflect_setPrototypeOf(target, proto) {
  $$reflectTarget(target, "setPrototypeOf");
  $$checkProto(proto);
  return $$setPrototypeOf(target, proto) === 0;
}
