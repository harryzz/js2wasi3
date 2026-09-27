// Arrays (T21 slice 3). Conventions: crates/lower/src/builtins.rs.
// `$$Array_prototype_x` is `Array.prototype.x`, `$$Array` is `Array(…)` / `new Array(…)`,
// `$$Array_from` is `Array.from`. The methods are the spec's generic algorithms (any array-like
// `this`); `push`, `pop`, `join`, `toString`, `values` are native (crates/backend/src/array.rs).
// Not implemented: species (results are always plain arrays), `Symbol.isConcatSpreadable`.
// Uses string.js's `$$toInteger`, `$$clamp`, `$$relative`, `$$String`.

// ---- helpers -----------------------------------------------------------------------------

// ToObject(this) (T30: primitives are wrapped), with node's message for `undefined`/`null`.
// Appends `v` to a prelude-made array (CreateDataProperty at `length`, T34): unlike `push`,
// never meets setters or read-only indices on the prototype chain.
function $$append(a, v) {
  __intrinsic_define(a, +a.length, v);
}

function $$thisObject(v, name) {
  if (v === undefined || v === null) {
    throw new TypeError("Array.prototype." + name + " called on null or undefined");
  }
  return __intrinsic_to_object(v);
}

// LengthOfArrayLike.
function $$lengthOf(o) {
  const n = $$toInteger(o.length);
  if (n <= 0) {
    return 0;
  }
  if (n > 9007199254740991) {
    return 9007199254740991;
  }
  return n;
}

// HasProperty(o, k) for an object or a primitive.
function $$has(o, k) {
  const t = typeof o;
  if (t === "object" || t === "function") {
    return k in o;
  }
  if (t === "string") {
    return k < +o.length;
  }
  return false;
}

// V8's name for a value that isn't callable, in "… is not a function" (node 24: every built-in's
// callback; also "… is not iterable").
function $$typeDescribe(v) {
  if (v === undefined) {
    return "undefined";
  }
  if (v === null) {
    return "object null";
  }
  const t = typeof v;
  if (t === "string") {
    return 'string "' + v + '"';
  }
  if (t === "number" || t === "boolean") {
    return t + " " + $$String(v);
  }
  if (t === "symbol" || t === "function") {
    return t;
  }
  return "object";
}

// How V8 names a receiver ("… called on incompatible receiver …") or a bad comparator.
function $$describe(v) {
  const t = typeof v;
  if (t === "object" && v !== null) {
    if (Array.isArray(v)) {
      return "[object Array]";
    }
    // Map/Set (T25): their prototypes' `constructor` is still a sentinel.
    const kind = __intrinsic_map_kind(v);
    if (kind >= 0) {
      return kind === 0 ? "#<Map>" : "#<Set>";
    }
    // Their iterators by `Symbol.toStringTag` (node 24).
    const mode = __intrinsic_map_iter_kind(v);
    if (mode >= 0) {
      return mode < 3 ? "[object Map Iterator]" : "[object Set Iterator]";
    }
    let p = Object.getPrototypeOf(v);
    if (p === null) {
      return "[object Object]";
    }
    // The nearest own `constructor` (built-in prototypes' other than Object's trap: F37).
    while (p !== Object.prototype && !Object.hasOwn(p, "constructor")) {
      p = Object.getPrototypeOf(p);
    }
    if (p === Object.prototype) {
      return "#<Object>";
    }
    const c = p.constructor;
    if (typeof c === "function" && c.name !== "") {
      return "#<" + c.name + ">";
    }
    return "#<Object>";
  }
  return $$String(v);
}

// Throw unless `f` is callable.
function $$callable(f) {
  if (typeof f !== "function") {
    throw new TypeError($$typeDescribe(f) + " is not a function");
  }
}

// Call a callback with `this` = thisArg.
function $$call3(f, thisArg, a, b, c) {
  if (thisArg === undefined) {
    return f(a, b, c);
  }
  return f.call(thisArg, a, b, c);
}

// ArrayCreate(n): a new array of n holes.
function $$newArray(n) {
  if (n > 4294967295) {
    throw new RangeError("Invalid array length");
  }
  const a = [];
  a.length = n;
  return a;
}

// SameValueZero.
function $$sameValueZero(x, y) {
  return x === y || (x !== x && y !== y);
}

// ---- constructors ------------------------------------------------------------------------

function $$Array(...args) {
  if (+args.length === 1) {
    const n = args[0];
    if (typeof n === "number") {
      if (n !== n >>> 0) {
        throw new RangeError("Invalid array length");
      }
      return $$newArray(n);
    }
  }
  const a = [];
  for (let i = 0; i < +args.length; i++) {
    __intrinsic_define(a, i, args[i]);
  }
  return a;
}

function $$Array_of(...items) {
  const a = [];
  for (let i = 0; i < +items.length; i++) {
    __intrinsic_define(a, i, items[i]);
  }
  return a;
}

function $$Array_from(items, mapfn, thisArg) {
  if (mapfn !== undefined && typeof mapfn !== "function") {
    throw new TypeError($$typeDescribe(mapfn) + " is not a function");
  }
  if (items === undefined || items === null) {
    throw new TypeError(
      "object " + items + " is not iterable (cannot read property Symbol(Symbol.iterator))",
    );
  }
  const a = [];
  if (typeof items === "string" || items[Symbol.iterator] !== undefined) {
    let k = 0;
    for (const v of items) {
      if (mapfn === undefined) {
        __intrinsic_define(a, k, v);
      } else {
        __intrinsic_define(a, k, $$call3(mapfn, thisArg, v, k, undefined));
      }
      k++;
    }
    return a;
  }
  const len = $$lengthOf(items);
  for (let k = 0; k < len; k++) {
    const v = items[k];
    if (mapfn === undefined) {
      __intrinsic_define(a, k, v);
    } else {
      __intrinsic_define(a, k, $$call3(mapfn, thisArg, v, k, undefined));
    }
  }
  return a;
}

// ---- iteration with a callback -----------------------------------------------------------

// Each element's `toLocaleString()`, joined with "," (`undefined`/`null`: empty), T25.
function $$Array_prototype_toLocaleString() {
  const o = $$thisObject(this, "toLocaleString");
  const len = $$lengthOf(o);
  let r = "";
  for (let k = 0; k < len; k++) {
    if (k > 0) {
      r += ",";
    }
    const e = o[k];
    if (e !== undefined && e !== null) {
      r += $$String(e.toLocaleString());
    }
  }
  return r;
}

function $$Array_prototype_forEach(callbackfn, thisArg) {
  const o = $$thisObject(this, "forEach");
  const len = $$lengthOf(o);
  $$callable(callbackfn);
  for (let k = 0; k < len; k++) {
    if ($$has(o, k)) {
      $$call3(callbackfn, thisArg, o[k], k, o);
    }
  }
  return undefined;
}

function $$Array_prototype_map(callbackfn, thisArg) {
  const o = $$thisObject(this, "map");
  const len = $$lengthOf(o);
  $$callable(callbackfn);
  const a = $$newArray(len);
  for (let k = 0; k < len; k++) {
    if ($$has(o, k)) {
      __intrinsic_define(a, k, $$call3(callbackfn, thisArg, o[k], k, o));
    }
  }
  return a;
}

function $$Array_prototype_filter(callbackfn, thisArg) {
  const o = $$thisObject(this, "filter");
  const len = $$lengthOf(o);
  $$callable(callbackfn);
  const a = [];
  let n = 0;
  for (let k = 0; k < len; k++) {
    if ($$has(o, k)) {
      const v = o[k];
      if ($$call3(callbackfn, thisArg, v, k, o)) {
        __intrinsic_define(a, n, v);
        n++;
      }
    }
  }
  return a;
}

function $$Array_prototype_some(callbackfn, thisArg) {
  const o = $$thisObject(this, "some");
  const len = $$lengthOf(o);
  $$callable(callbackfn);
  for (let k = 0; k < len; k++) {
    if ($$has(o, k) && $$call3(callbackfn, thisArg, o[k], k, o)) {
      return true;
    }
  }
  return false;
}

function $$Array_prototype_every(callbackfn, thisArg) {
  const o = $$thisObject(this, "every");
  const len = $$lengthOf(o);
  $$callable(callbackfn);
  for (let k = 0; k < len; k++) {
    if ($$has(o, k) && !$$call3(callbackfn, thisArg, o[k], k, o)) {
      return false;
    }
  }
  return true;
}

function $$reduce(o, len, callbackfn, args, right) {
  $$callable(callbackfn);
  let k = right ? len - 1 : 0;
  const step = right ? -1 : 1;
  let acc;
  if (+args.length > 0) {
    acc = args[0];
  } else {
    let found = false;
    while (!found && k >= 0 && k < len) {
      if ($$has(o, k)) {
        acc = o[k];
        found = true;
      }
      k += step;
    }
    if (!found) {
      throw new TypeError("Reduce of empty array with no initial value");
    }
  }
  for (; k >= 0 && k < len; k += step) {
    if ($$has(o, k)) {
      acc = callbackfn(acc, o[k], k, o);
    }
  }
  return acc;
}

function $$Array_prototype_reduce(callbackfn, ...initialValue) {
  const o = $$thisObject(this, "reduce");
  return $$reduce(o, $$lengthOf(o), callbackfn, initialValue, false);
}

function $$Array_prototype_reduceRight(callbackfn, ...initialValue) {
  const o = $$thisObject(this, "reduceRight");
  return $$reduce(o, $$lengthOf(o), callbackfn, initialValue, true);
}

// find / findIndex / findLast / findLastIndex (holes are read as undefined).
function $$find(o, predicate, thisArg, fromEnd, wantIndex) {
  const len = $$lengthOf(o);
  $$callable(predicate);
  for (let i = 0; i < len; i++) {
    const k = fromEnd ? len - 1 - i : i;
    const v = o[k];
    if ($$call3(predicate, thisArg, v, k, o)) {
      return wantIndex ? k : v;
    }
  }
  return wantIndex ? -1 : undefined;
}

function $$Array_prototype_find(predicate, thisArg) {
  return $$find($$thisObject(this, "find"), predicate, thisArg, false, false);
}

function $$Array_prototype_findIndex(predicate, thisArg) {
  return $$find($$thisObject(this, "findIndex"), predicate, thisArg, false, true);
}

function $$Array_prototype_findLast(predicate, thisArg) {
  return $$find($$thisObject(this, "findLast"), predicate, thisArg, true, false);
}

function $$Array_prototype_findLastIndex(predicate, thisArg) {
  return $$find($$thisObject(this, "findLastIndex"), predicate, thisArg, true, true);
}

// ---- searching ---------------------------------------------------------------------------

function $$Array_prototype_indexOf(searchElement, fromIndex) {
  const o = $$thisObject(this, "indexOf");
  const len = $$lengthOf(o);
  if (len === 0) {
    return -1;
  }
  for (let k = $$relative($$toInteger(fromIndex), len); k < len; k++) {
    if ($$has(o, k) && o[k] === searchElement) {
      return k;
    }
  }
  return -1;
}

function $$Array_prototype_lastIndexOf(searchElement, ...fromIndex) {
  const o = $$thisObject(this, "lastIndexOf");
  const len = $$lengthOf(o);
  if (len === 0) {
    return -1;
  }
  let k = len - 1;
  if (+fromIndex.length > 0) {
    const n = $$toInteger(fromIndex[0]);
    k = n < 0 ? len + n : n > len - 1 ? len - 1 : n;
  }
  for (; k >= 0; k--) {
    if ($$has(o, k) && o[k] === searchElement) {
      return k;
    }
  }
  return -1;
}

function $$Array_prototype_includes(searchElement, fromIndex) {
  const o = $$thisObject(this, "includes");
  const len = $$lengthOf(o);
  for (let k = $$relative($$toInteger(fromIndex), len); k < len; k++) {
    if ($$sameValueZero(o[k], searchElement)) {
      return true;
    }
  }
  return false;
}

function $$Array_prototype_at(index) {
  const o = $$thisObject(this, "at");
  const len = $$lengthOf(o);
  let k = $$toInteger(index);
  if (k < 0) {
    k = len + k;
  }
  if (k < 0 || k >= len) {
    return undefined;
  }
  return o[k];
}

// ---- copying -----------------------------------------------------------------------------

function $$Array_prototype_slice(start, end) {
  const o = $$thisObject(this, "slice");
  const len = $$lengthOf(o);
  const from = $$relative($$toInteger(start), len);
  let to = len;
  if (end !== undefined) {
    to = $$relative($$toInteger(end), len);
  }
  const a = [];
  let n = 0;
  for (let k = from; k < to; k++) {
    if ($$has(o, k)) {
      __intrinsic_define(a, n, o[k]);
    }
    n++;
  }
  a.length = n;
  return a;
}

function $$Array_prototype_concat(...items) {
  const o = $$thisObject(this, "concat");
  const a = [];
  let n = 0;
  for (let i = -1; i < +items.length; i++) {
    const e = i < 0 ? o : items[i];
    if (Array.isArray(e)) {
      const len = +e.length;
      for (let k = 0; k < len; k++) {
        if (k in e) {
          __intrinsic_define(a, n, e[k]);
        }
        n++;
      }
    } else {
      __intrinsic_define(a, n, e);
      n++;
    }
  }
  a.length = n;
  return a;
}

// FlattenIntoArray: append the elements of `source` to `target` from `start`; the next index.
function $$flatten(target, source, start, depth, mapper, thisArg) {
  const len = $$lengthOf(source);
  let n = start;
  for (let k = 0; k < len; k++) {
    if ($$has(source, k)) {
      let e = source[k];
      if (mapper !== undefined) {
        e = $$call3(mapper, thisArg, e, k, source);
      }
      if (depth > 0 && Array.isArray(e)) {
        n = $$flatten(target, e, n, depth - 1, undefined, undefined);
      } else {
        __intrinsic_define(target, n, e);
        n++;
      }
    }
  }
  return n;
}

function $$Array_prototype_flat(depth) {
  const o = $$thisObject(this, "flat");
  let d = 1;
  if (depth !== undefined) {
    d = $$toInteger(depth);
  }
  const a = [];
  $$flatten(a, o, 0, d, undefined, undefined);
  return a;
}

function $$Array_prototype_flatMap(mapper, thisArg) {
  const o = $$thisObject(this, "flatMap");
  if (typeof mapper !== "function") {
    throw new TypeError($$typeDescribe(mapper) + " is not a function");
  }
  const a = [];
  $$flatten(a, o, 0, 1, mapper, thisArg);
  return a;
}

// ---- in place ----------------------------------------------------------------------------

function $$Array_prototype_fill(value, start, end) {
  const o = $$thisObject(this, "fill");
  const len = $$lengthOf(o);
  let k = $$relative($$toInteger(start), len);
  let to = len;
  if (end !== undefined) {
    to = $$relative($$toInteger(end), len);
  }
  for (; k < to; k++) {
    o[k] = value;
  }
  return o;
}

function $$Array_prototype_copyWithin(target, start, end) {
  const o = $$thisObject(this, "copyWithin");
  const len = $$lengthOf(o);
  let to = $$relative($$toInteger(target), len);
  let from = $$relative($$toInteger(start), len);
  let fin = len;
  if (end !== undefined) {
    fin = $$relative($$toInteger(end), len);
  }
  let count = fin - from;
  if (len - to < count) {
    count = len - to;
  }
  let dir = 1;
  if (from < to && to < from + count) {
    dir = -1;
    from = from + count - 1;
    to = to + count - 1;
  }
  for (; count > 0; count--) {
    if ($$has(o, from)) {
      o[to] = o[from];
    } else {
      delete o[to];
    }
    from += dir;
    to += dir;
  }
  return o;
}

function $$Array_prototype_reverse() {
  const o = $$thisObject(this, "reverse");
  const len = $$lengthOf(o);
  const middle = Math.floor(len / 2);
  for (let lower = 0; lower < middle; lower++) {
    const upper = len - lower - 1;
    const lowerExists = $$has(o, lower);
    const upperExists = $$has(o, upper);
    const lowerValue = o[lower];
    const upperValue = o[upper];
    if (upperExists) {
      o[lower] = upperValue;
    } else {
      delete o[lower];
    }
    if (lowerExists) {
      o[upper] = lowerValue;
    } else {
      delete o[upper];
    }
  }
  return o;
}

function $$Array_prototype_shift() {
  const o = $$thisObject(this, "shift");
  const len = $$lengthOf(o);
  if (len === 0) {
    o.length = 0;
    return undefined;
  }
  const first = o[0];
  for (let k = 1; k < len; k++) {
    if ($$has(o, k)) {
      o[k - 1] = o[k];
    } else {
      delete o[k - 1];
    }
  }
  delete o[len - 1];
  o.length = len - 1;
  return first;
}

function $$Array_prototype_unshift(...items) {
  const o = $$thisObject(this, "unshift");
  const len = $$lengthOf(o);
  const n = +items.length;
  if (n > 0) {
    for (let k = len; k > 0; k--) {
      if ($$has(o, k - 1)) {
        o[k + n - 1] = o[k - 1];
      } else {
        delete o[k + n - 1];
      }
    }
    for (let j = 0; j < n; j++) {
      o[j] = items[j];
    }
  }
  o.length = len + n;
  return len + n;
}

function $$Array_prototype_splice(...args) {
  const o = $$thisObject(this, "splice");
  const len = $$lengthOf(o);
  const start = $$relative($$toInteger(args[0]), len);
  let deleteCount = 0;
  if (+args.length === 1) {
    deleteCount = len - start;
  } else if (+args.length > 1) {
    deleteCount = $$clamp($$toInteger(args[1]), len - start);
  }
  const itemCount = +args.length > 2 ? +args.length - 2 : 0;
  const removed = $$newArray(deleteCount);
  for (let k = 0; k < deleteCount; k++) {
    if ($$has(o, start + k)) {
      __intrinsic_define(removed, k, o[start + k]);
    }
  }
  if (itemCount < deleteCount) {
    for (let k = start; k < len - deleteCount; k++) {
      if ($$has(o, k + deleteCount)) {
        o[k + itemCount] = o[k + deleteCount];
      } else {
        delete o[k + itemCount];
      }
    }
    for (let k = len; k > len - deleteCount + itemCount; k--) {
      delete o[k - 1];
    }
  } else if (itemCount > deleteCount) {
    for (let k = len - deleteCount; k > start; k--) {
      if ($$has(o, k + deleteCount - 1)) {
        o[k + itemCount - 1] = o[k + deleteCount - 1];
      } else {
        delete o[k + itemCount - 1];
      }
    }
  }
  for (let i = 0; i < itemCount; i++) {
    o[start + i] = args[i + 2];
  }
  o.length = len - deleteCount + itemCount;
  return removed;
}

// ---- sorting -----------------------------------------------------------------------------

// A stable merge sort of `v` (an array without holes or undefined). `keys`: the values' strings
// (only when comparefn is undefined and ToString of every value is unobservable), or undefined.
function $$mergeSort(v, keys, comparefn) {
  const n = +v.length;
  let src = v;
  let dst = $$newArray(n);
  let ksrc = keys;
  let kdst = keys === undefined ? undefined : $$newArray(n);
  for (let width = 1; width < n; width *= 2) {
    for (let lo = 0; lo < n; lo += 2 * width) {
      const mid = lo + width < n ? lo + width : n;
      const hi = lo + 2 * width < n ? lo + 2 * width : n;
      let i = lo;
      let j = mid;
      for (let k = lo; k < hi; k++) {
        let left = j >= hi;
        if (!left && i < mid) {
          // SortCompare(right, left) >= 0 (NaN is 0).
          if (keys !== undefined) {
            left = !(ksrc[j] < ksrc[i]);
          } else if (comparefn !== undefined) {
            left = !(+comparefn(src[j], src[i]) < 0);
          } else {
            left = !(__intrinsic_to_string(src[j]) < __intrinsic_to_string(src[i]));
          }
        }
        if (left) {
          __intrinsic_define(dst, k, src[i]);
          if (keys !== undefined) {
            __intrinsic_define(kdst, k, ksrc[i]);
          }
          i++;
        } else {
          __intrinsic_define(dst, k, src[j]);
          if (keys !== undefined) {
            __intrinsic_define(kdst, k, ksrc[j]);
          }
          j++;
        }
      }
    }
    const t = src;
    src = dst;
    dst = t;
    const kt = ksrc;
    ksrc = kdst;
    kdst = kt;
  }
  return src;
}

// SortIndexedProperties: the values of `o[0..len)` sorted (holes skipped when `skipHoles`),
// undefined last.
function $$sortValues(o, len, comparefn, skipHoles) {
  const v = [];
  let n = 0;
  let undefs = 0;
  let primitive = comparefn === undefined;
  for (let k = 0; k < len; k++) {
    if (!skipHoles || $$has(o, k)) {
      const x = o[k];
      if (x === undefined) {
        undefs++;
      } else {
        if (primitive && (typeof x === "object" || typeof x === "function" || typeof x === "symbol")) {
          primitive = false;
        }
        __intrinsic_define(v, n, x);
        n++;
      }
    }
  }
  let keys = undefined;
  if (primitive) {
    keys = $$newArray(n);
    for (let k = 0; k < n; k++) {
      __intrinsic_define(keys, k, __intrinsic_to_string(v[k]));
    }
  }
  const r = $$mergeSort(v, keys, comparefn);
  for (let k = 0; k < undefs; k++) {
    __intrinsic_define(r, n + k, undefined);
  }
  return r;
}

function $$checkCompare(comparefn) {
  if (comparefn !== undefined && typeof comparefn !== "function") {
    throw new TypeError(
      "The comparison function must be either a function or undefined: " + $$describe(comparefn),
    );
  }
}

function $$Array_prototype_sort(comparefn) {
  $$checkCompare(comparefn);
  const o = $$thisObject(this, "sort");
  const len = $$lengthOf(o);
  const sorted = $$sortValues(o, len, comparefn, true);
  const n = +sorted.length;
  for (let k = 0; k < n; k++) {
    o[k] = sorted[k];
  }
  for (let k = n; k < len; k++) {
    if ($$has(o, k)) {
      delete o[k];
    }
  }
  return o;
}

// ---- copying variants (ES2023) -----------------------------------------------------------

function $$Array_prototype_toSorted(comparefn) {
  $$checkCompare(comparefn);
  const o = $$thisObject(this, "toSorted");
  return $$sortValues(o, $$lengthOf(o), comparefn, false);
}

function $$Array_prototype_toReversed() {
  const o = $$thisObject(this, "toReversed");
  const len = $$lengthOf(o);
  const a = [];
  for (let k = 0; k < len; k++) {
    __intrinsic_define(a, k, o[len - 1 - k]);
  }
  return a;
}

function $$Array_prototype_with(index, value) {
  const o = $$thisObject(this, "with");
  const len = $$lengthOf(o);
  let k = $$toInteger(index);
  if (k < 0) {
    k = len + k;
  }
  if (k < 0 || k >= len) {
    throw new RangeError("Invalid index : " + index);
  }
  const a = [];
  for (let i = 0; i < len; i++) {
    __intrinsic_define(a, i, i === k ? value : o[i]);
  }
  return a;
}

function $$Array_prototype_toSpliced(...args) {
  const o = $$thisObject(this, "toSpliced");
  const len = $$lengthOf(o);
  const start = $$relative($$toInteger(args[0]), len);
  let skip = 0;
  if (+args.length === 1) {
    skip = len - start;
  } else if (+args.length > 1) {
    skip = $$clamp($$toInteger(args[1]), len - start);
  }
  const a = [];
  let n = 0;
  for (let k = 0; k < start; k++) {
    __intrinsic_define(a, n, o[k]);
    n++;
  }
  for (let i = 2; i < +args.length; i++) {
    __intrinsic_define(a, n, args[i]);
    n++;
  }
  for (let k = start + skip; k < len; k++) {
    __intrinsic_define(a, n, o[k]);
    n++;
  }
  return a;
}
