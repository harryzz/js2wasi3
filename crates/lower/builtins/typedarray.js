// Typed arrays and ArrayBuffer (T20 slice 1c). Conventions: crates/lower/src/builtins.rs.
// The representation and element access are native (crates/backend/src/typed.rs,
// `__intrinsic_typed_*`); `$$Uint8Array` is `new Uint8Array(…)` (calling it without `new`
// throws in the lowering), `$$TypedArray_prototype_x` is `%TypedArray%.prototype.x` (shared by
// the nine kinds, whose prototypes inherit from it). `values`/`keys`/`entries` and
// `[Symbol.iterator]` are the native array iterators.
// Kinds: 0 Int8, 1 Uint8, 2 Uint8Clamped, 3 Int16, 4 Uint16, 5 Int32, 6 Uint32, 7 Float32,
// 8 Float64. Not implemented: Float16Array, BigInt64Array/BigUint64Array, species and
// subclassing (results have the receiver's kind), resizable/transferable buffers, `Object.keys`
// and `for-in` over the elements, `DataView`.
// Uses string.js's `$$toInteger`, `$$relative`, array.js's `$$lengthOf`, `$$callable`,
// `$$call3`, `$$sameValueZero`, `$$mergeSort`, `$$describe`.

// ---- helpers -----------------------------------------------------------------------------

function $$typedName(kind) {
  if (kind === 0) {
    return "Int8Array";
  }
  if (kind === 1) {
    return "Uint8Array";
  }
  if (kind === 2) {
    return "Uint8ClampedArray";
  }
  if (kind === 3) {
    return "Int16Array";
  }
  if (kind === 4) {
    return "Uint16Array";
  }
  if (kind === 5) {
    return "Int32Array";
  }
  if (kind === 6) {
    return "Uint32Array";
  }
  if (kind === 7) {
    return "Float32Array";
  }
  return "Float64Array";
}

function $$typedSize(kind) {
  if (kind < 3) {
    return 1;
  }
  if (kind < 5) {
    return 2;
  }
  if (kind < 8) {
    return 4;
  }
  return 8;
}

// ValidateTypedArray(this): its kind.
function $$typedThis(v) {
  const kind = __intrinsic_typed_kind(v);
  if (kind < 0) {
    throw new TypeError("this is not a typed array.");
  }
  return kind;
}

// A new typed array of `kind` and `length` (a checked integer), zeroed.
function $$typedAlloc(kind, length) {
  if (length < 0 || length > 9007199254740991) {
    throw new RangeError("Invalid typed array length: " + length);
  }
  if (length * $$typedSize(kind) > 2147483647) {
    throw new RangeError("Array buffer allocation failed");
  }
  return __intrinsic_typed_new(kind, length);
}

// ToIndex for a length argument.
function $$typedLength(v) {
  if (v === undefined) {
    return 0;
  }
  const n = $$toInteger(v);
  if (n < 0 || n > 9007199254740991) {
    throw new RangeError("Invalid typed array length: " + n);
  }
  return n;
}

// The typed array constructors: from a length, an ArrayBuffer (offset, length), a typed array,
// an iterable or an array-like.
function $$typedCreate(kind, a, b, c) {
  const name = $$typedName(kind);
  const t = typeof a;
  if (a === null || (t !== "object" && t !== "function")) {
    return $$typedAlloc(kind, $$typedLength(a));
  }
  if (__intrinsic_is_array_buffer(a)) {
    const size = $$typedSize(kind);
    const bytes = __intrinsic_buffer_length(a);
    const offset = $$toInteger(b);
    if (offset < 0 || offset > 9007199254740991) {
      throw new RangeError("Start offset " + offset + " is outside the bounds of the buffer");
    }
    if (offset % size !== 0) {
      throw new RangeError("start offset of " + name + " should be a multiple of " + size);
    }
    let length;
    if (c === undefined) {
      if (bytes % size !== 0) {
        throw new RangeError("byte length of " + name + " should be a multiple of " + size);
      }
      if (offset > bytes) {
        throw new RangeError("Start offset " + offset + " is outside the bounds of the buffer");
      }
      length = (bytes - offset) / size;
    } else {
      length = $$typedLength(c);
      if (offset + length * size > bytes) {
        throw new RangeError("Invalid typed array length: " + length);
      }
    }
    return __intrinsic_typed_view(kind, a, offset, length);
  }
  if (__intrinsic_typed_kind(a) >= 0) {
    const n = __intrinsic_typed_length(a);
    const r = $$typedAlloc(kind, n);
    for (let i = 0; i < n; i++) {
      r[i] = a[i];
    }
    return r;
  }
  const iter = a[Symbol.iterator];
  let src = a;
  if (iter !== undefined && iter !== null) {
    $$callable(iter);
    src = [...a];
  }
  const n = $$lengthOf(src);
  const r = $$typedAlloc(kind, n);
  for (let i = 0; i < n; i++) {
    r[i] = src[i];
  }
  return r;
}

function $$Int8Array(a, b, c) {
  return $$typedCreate(0, a, b, c);
}
function $$Uint8Array(a, b, c) {
  return $$typedCreate(1, a, b, c);
}
function $$Uint8ClampedArray(a, b, c) {
  return $$typedCreate(2, a, b, c);
}
function $$Int16Array(a, b, c) {
  return $$typedCreate(3, a, b, c);
}
function $$Uint16Array(a, b, c) {
  return $$typedCreate(4, a, b, c);
}
function $$Int32Array(a, b, c) {
  return $$typedCreate(5, a, b, c);
}
function $$Uint32Array(a, b, c) {
  return $$typedCreate(6, a, b, c);
}
function $$Float32Array(a, b, c) {
  return $$typedCreate(7, a, b, c);
}
function $$Float64Array(a, b, c) {
  return $$typedCreate(8, a, b, c);
}

// %TypedArray%.from(source, mapfn, thisArg) for one kind.
function $$typedFrom(kind, source, rest) {
  const mapfn = rest[0];
  if (mapfn !== undefined) {
    $$callable(mapfn);
  }
  const iter = source === undefined || source === null ? undefined : source[Symbol.iterator];
  const src = iter !== undefined && iter !== null ? [...source] : source;
  if (src === undefined || src === null) {
    throw new TypeError("Cannot convert undefined or null to object");
  }
  const n = $$lengthOf(src);
  const r = $$typedAlloc(kind, n);
  for (let i = 0; i < n; i++) {
    r[i] = mapfn === undefined ? src[i] : $$call3(mapfn, rest[1], src[i], i, undefined);
  }
  return r;
}

// %TypedArray%.of(...items) for one kind.
function $$typedOf(kind, items) {
  const n = +items.length;
  const r = $$typedAlloc(kind, n);
  for (let i = 0; i < n; i++) {
    r[i] = items[i];
  }
  return r;
}

function $$Int8Array_from(source, ...rest) {
  return $$typedFrom(0, source, rest);
}
function $$Uint8Array_from(source, ...rest) {
  return $$typedFrom(1, source, rest);
}
function $$Uint8ClampedArray_from(source, ...rest) {
  return $$typedFrom(2, source, rest);
}
function $$Int16Array_from(source, ...rest) {
  return $$typedFrom(3, source, rest);
}
function $$Uint16Array_from(source, ...rest) {
  return $$typedFrom(4, source, rest);
}
function $$Int32Array_from(source, ...rest) {
  return $$typedFrom(5, source, rest);
}
function $$Uint32Array_from(source, ...rest) {
  return $$typedFrom(6, source, rest);
}
function $$Float32Array_from(source, ...rest) {
  return $$typedFrom(7, source, rest);
}
function $$Float64Array_from(source, ...rest) {
  return $$typedFrom(8, source, rest);
}
function $$Int8Array_of(...items) {
  return $$typedOf(0, items);
}
function $$Uint8Array_of(...items) {
  return $$typedOf(1, items);
}
function $$Uint8ClampedArray_of(...items) {
  return $$typedOf(2, items);
}
function $$Int16Array_of(...items) {
  return $$typedOf(3, items);
}
function $$Uint16Array_of(...items) {
  return $$typedOf(4, items);
}
function $$Int32Array_of(...items) {
  return $$typedOf(5, items);
}
function $$Uint32Array_of(...items) {
  return $$typedOf(6, items);
}
function $$Float32Array_of(...items) {
  return $$typedOf(7, items);
}
function $$Float64Array_of(...items) {
  return $$typedOf(8, items);
}

// ---- %TypedArray%.prototype getters ------------------------------------------------------

function $$TypedArray_prototype_length() {
  $$typedThis(this);
  return __intrinsic_typed_length(this);
}

function $$TypedArray_prototype_byteLength() {
  const kind = $$typedThis(this);
  return __intrinsic_typed_length(this) * $$typedSize(kind);
}

function $$TypedArray_prototype_byteOffset() {
  $$typedThis(this);
  return __intrinsic_typed_byte_offset(this);
}

function $$TypedArray_prototype_buffer() {
  $$typedThis(this);
  return __intrinsic_typed_buffer(this);
}

// ---- %TypedArray%.prototype methods ------------------------------------------------------

function $$TypedArray_prototype_at(index) {
  $$typedThis(this);
  const len = __intrinsic_typed_length(this);
  const rel = $$toInteger(index);
  const k = rel >= 0 ? rel : len + rel;
  if (k < 0 || k >= len) {
    return undefined;
  }
  return this[k];
}

function $$TypedArray_prototype_forEach(callbackfn, thisArg) {
  $$typedThis(this);
  $$callable(callbackfn);
  const len = __intrinsic_typed_length(this);
  for (let k = 0; k < len; k++) {
    $$call3(callbackfn, thisArg, this[k], k, this);
  }
  return undefined;
}

function $$TypedArray_prototype_map(callbackfn, thisArg) {
  const kind = $$typedThis(this);
  $$callable(callbackfn);
  const len = __intrinsic_typed_length(this);
  const r = __intrinsic_typed_new(kind, len);
  for (let k = 0; k < len; k++) {
    r[k] = $$call3(callbackfn, thisArg, this[k], k, this);
  }
  return r;
}

function $$TypedArray_prototype_filter(callbackfn, thisArg) {
  const kind = $$typedThis(this);
  $$callable(callbackfn);
  const len = __intrinsic_typed_length(this);
  const kept = [];
  let n = 0;
  for (let k = 0; k < len; k++) {
    const v = this[k];
    if ($$call3(callbackfn, thisArg, v, k, this)) {
      kept[n] = v;
      n++;
    }
  }
  const r = __intrinsic_typed_new(kind, n);
  for (let k = 0; k < n; k++) {
    r[k] = kept[k];
  }
  return r;
}

function $$TypedArray_prototype_some(callbackfn, thisArg) {
  $$typedThis(this);
  $$callable(callbackfn);
  const len = __intrinsic_typed_length(this);
  for (let k = 0; k < len; k++) {
    if ($$call3(callbackfn, thisArg, this[k], k, this)) {
      return true;
    }
  }
  return false;
}

function $$TypedArray_prototype_every(callbackfn, thisArg) {
  $$typedThis(this);
  $$callable(callbackfn);
  const len = __intrinsic_typed_length(this);
  for (let k = 0; k < len; k++) {
    if (!$$call3(callbackfn, thisArg, this[k], k, this)) {
      return false;
    }
  }
  return true;
}

function $$typedReduce(o, callbackfn, args, right) {
  $$callable(callbackfn);
  const len = __intrinsic_typed_length(o);
  let k = right ? len - 1 : 0;
  const step = right ? -1 : 1;
  let acc;
  if (+args.length > 0) {
    acc = args[0];
  } else {
    if (len === 0) {
      throw new TypeError("Reduce of empty array with no initial value");
    }
    acc = o[k];
    k += step;
  }
  for (; k >= 0 && k < len; k += step) {
    acc = callbackfn(acc, o[k], k, o);
  }
  return acc;
}

function $$TypedArray_prototype_reduce(callbackfn, ...initialValue) {
  $$typedThis(this);
  return $$typedReduce(this, callbackfn, initialValue, false);
}

function $$TypedArray_prototype_reduceRight(callbackfn, ...initialValue) {
  $$typedThis(this);
  return $$typedReduce(this, callbackfn, initialValue, true);
}

function $$typedFind(o, predicate, thisArg, fromEnd, wantIndex) {
  $$callable(predicate);
  const len = __intrinsic_typed_length(o);
  for (let i = 0; i < len; i++) {
    const k = fromEnd ? len - 1 - i : i;
    const v = o[k];
    if ($$call3(predicate, thisArg, v, k, o)) {
      return wantIndex ? k : v;
    }
  }
  return wantIndex ? -1 : undefined;
}

function $$TypedArray_prototype_find(predicate, thisArg) {
  $$typedThis(this);
  return $$typedFind(this, predicate, thisArg, false, false);
}

function $$TypedArray_prototype_findIndex(predicate, thisArg) {
  $$typedThis(this);
  return $$typedFind(this, predicate, thisArg, false, true);
}

function $$TypedArray_prototype_findLast(predicate, thisArg) {
  $$typedThis(this);
  return $$typedFind(this, predicate, thisArg, true, false);
}

function $$TypedArray_prototype_findLastIndex(predicate, thisArg) {
  $$typedThis(this);
  return $$typedFind(this, predicate, thisArg, true, true);
}

function $$TypedArray_prototype_indexOf(searchElement, fromIndex) {
  $$typedThis(this);
  const len = __intrinsic_typed_length(this);
  for (let k = $$relative($$toInteger(fromIndex), len); k < len; k++) {
    if (this[k] === searchElement) {
      return k;
    }
  }
  return -1;
}

function $$TypedArray_prototype_lastIndexOf(searchElement, ...fromIndex) {
  $$typedThis(this);
  const len = __intrinsic_typed_length(this);
  let k = len - 1;
  if (+fromIndex.length > 0) {
    const n = $$toInteger(fromIndex[0]);
    k = n >= 0 ? (n < len - 1 ? n : len - 1) : len + n;
  }
  for (; k >= 0; k--) {
    if (this[k] === searchElement) {
      return k;
    }
  }
  return -1;
}

function $$TypedArray_prototype_includes(searchElement, fromIndex) {
  $$typedThis(this);
  const len = __intrinsic_typed_length(this);
  for (let k = $$relative($$toInteger(fromIndex), len); k < len; k++) {
    if ($$sameValueZero(this[k], searchElement)) {
      return true;
    }
  }
  return false;
}

function $$typedJoin(o, separator) {
  const sep = separator === undefined ? "," : __intrinsic_to_string(separator);
  const len = __intrinsic_typed_length(o);
  let r = "";
  for (let k = 0; k < len; k++) {
    if (k > 0) {
      r += sep;
    }
    r += __intrinsic_to_string(o[k]);
  }
  return r;
}

function $$TypedArray_prototype_join(separator) {
  $$typedThis(this);
  return $$typedJoin(this, separator);
}

function $$TypedArray_prototype_toString() {
  $$typedThis(this);
  return $$typedJoin(this, undefined);
}

function $$TypedArray_prototype_fill(value, start, end) {
  $$typedThis(this);
  const len = __intrinsic_typed_length(this);
  const v = +value;
  const from = $$relative($$toInteger(start), len);
  const to = end === undefined ? len : $$relative($$toInteger(end), len);
  for (let k = from; k < to; k++) {
    this[k] = v;
  }
  return this;
}

function $$TypedArray_prototype_reverse() {
  $$typedThis(this);
  const len = __intrinsic_typed_length(this);
  for (let lo = 0, hi = len - 1; lo < hi; lo++, hi--) {
    const t = this[lo];
    this[lo] = this[hi];
    this[hi] = t;
  }
  return this;
}

function $$TypedArray_prototype_copyWithin(target, start, end) {
  const kind = $$typedThis(this);
  const len = __intrinsic_typed_length(this);
  const to = $$relative($$toInteger(target), len);
  const from = $$relative($$toInteger(start), len);
  const last = end === undefined ? len : $$relative($$toInteger(end), len);
  let count = last - from;
  if (len - to < count) {
    count = len - to;
  }
  if (count > 0) {
    __intrinsic_typed_copy(this, to, this, from, count);
  }
  return this;
}

// `set(source, offset)`: a typed array (the same kind: bytes copied) or an array-like.
function $$TypedArray_prototype_set(source, offset) {
  const kind = $$typedThis(this);
  const off = $$toInteger(offset);
  if (off < 0) {
    throw new RangeError("offset is out of bounds");
  }
  const len = __intrinsic_typed_length(this);
  const skind = __intrinsic_typed_kind(source);
  if (skind >= 0) {
    const n = __intrinsic_typed_length(source);
    if (n + off > len) {
      throw new RangeError("offset is out of bounds");
    }
    if (skind === kind) {
      __intrinsic_typed_copy(this, off, source, 0, n);
    } else {
      const copy = [...source];
      for (let k = 0; k < n; k++) {
        this[off + k] = copy[k];
      }
    }
    return undefined;
  }
  if (source === undefined || source === null) {
    throw new TypeError("Cannot convert undefined or null to object");
  }
  const n = $$lengthOf(source);
  if (n + off > len) {
    throw new RangeError("offset is out of bounds");
  }
  for (let k = 0; k < n; k++) {
    this[off + k] = source[k];
  }
  return undefined;
}

function $$TypedArray_prototype_slice(start, end) {
  const kind = $$typedThis(this);
  const len = __intrinsic_typed_length(this);
  const from = $$relative($$toInteger(start), len);
  const to = end === undefined ? len : $$relative($$toInteger(end), len);
  const n = to > from ? to - from : 0;
  const r = __intrinsic_typed_new(kind, n);
  if (n > 0) {
    __intrinsic_typed_copy(r, 0, this, from, n);
  }
  return r;
}

function $$TypedArray_prototype_subarray(start, end) {
  const kind = $$typedThis(this);
  const len = __intrinsic_typed_length(this);
  const from = $$relative($$toInteger(start), len);
  const to = end === undefined ? len : $$relative($$toInteger(end), len);
  const n = to > from ? to - from : 0;
  const offset = __intrinsic_typed_byte_offset(this) + from * $$typedSize(kind);
  return __intrinsic_typed_view(kind, __intrinsic_typed_buffer(this), offset, n);
}

// The default order: numeric, -0 before +0, NaN last.
function $$typedCompare(x, y) {
  if (x !== x) {
    return y !== y ? 0 : 1;
  }
  if (y !== y) {
    return -1;
  }
  if (x < y) {
    return -1;
  }
  if (x > y) {
    return 1;
  }
  if (x === 0 && y === 0) {
    const nx = 1 / x < 0;
    const ny = 1 / y < 0;
    return nx === ny ? 0 : nx ? -1 : 1;
  }
  return 0;
}

function $$typedSorted(o, comparefn) {
  if (comparefn !== undefined && typeof comparefn !== "function") {
    throw new TypeError(
      "The comparison function must be either a function or undefined: " + $$describe(comparefn),
    );
  }
  const len = __intrinsic_typed_length(o);
  const v = [];
  for (let k = 0; k < len; k++) {
    v[k] = o[k];
  }
  return $$mergeSort(v, undefined, comparefn === undefined ? $$typedCompare : comparefn);
}

function $$TypedArray_prototype_sort(comparefn) {
  $$typedThis(this);
  const sorted = $$typedSorted(this, comparefn);
  const len = +sorted.length;
  for (let k = 0; k < len; k++) {
    this[k] = sorted[k];
  }
  return this;
}

function $$TypedArray_prototype_toSorted(comparefn) {
  const kind = $$typedThis(this);
  const sorted = $$typedSorted(this, comparefn);
  const len = +sorted.length;
  const r = __intrinsic_typed_new(kind, len);
  for (let k = 0; k < len; k++) {
    r[k] = sorted[k];
  }
  return r;
}

function $$TypedArray_prototype_toReversed() {
  const kind = $$typedThis(this);
  const len = __intrinsic_typed_length(this);
  const r = __intrinsic_typed_new(kind, len);
  for (let k = 0; k < len; k++) {
    r[k] = this[len - 1 - k];
  }
  return r;
}

function $$TypedArray_prototype_with(index, value) {
  const kind = $$typedThis(this);
  const len = __intrinsic_typed_length(this);
  const rel = $$toInteger(index);
  const k = rel >= 0 ? rel : len + rel;
  const v = +value;
  if (k < 0 || k >= len) {
    throw new RangeError("Invalid typed array index");
  }
  const r = __intrinsic_typed_new(kind, len);
  __intrinsic_typed_copy(r, 0, this, 0, len);
  r[k] = v;
  return r;
}

// ---- ArrayBuffer -------------------------------------------------------------------------

function $$ArrayBuffer(length) {
  const n = $$toInteger(length);
  if (n < 0 || n > 9007199254740991) {
    throw new RangeError("Invalid array buffer length");
  }
  if (n > 2147483647) {
    throw new RangeError("Array buffer allocation failed");
  }
  return __intrinsic_buffer_new(n);
}

function $$ArrayBuffer_isView(arg) {
  return __intrinsic_typed_kind(arg) >= 0;
}

function $$bufferThis(v, name) {
  if (!__intrinsic_is_array_buffer(v)) {
    throw new TypeError(
      "Method ArrayBuffer.prototype." + name + " called on incompatible receiver " + $$describe(v),
    );
  }
}

function $$ArrayBuffer_prototype_byteLength() {
  $$bufferThis(this, "byteLength");
  return __intrinsic_buffer_length(this);
}

function $$ArrayBuffer_prototype_slice(start, end) {
  $$bufferThis(this, "slice");
  const len = __intrinsic_buffer_length(this);
  const from = $$relative($$toInteger(start), len);
  const to = end === undefined ? len : $$relative($$toInteger(end), len);
  return __intrinsic_buffer_slice(this, from, to > from ? to : from);
}
