// node:internal/dataview (T64): `DataView`, an ECMAScript built-in that js2wasi3 provides like node's
// globals bound by the linker (`node::GLOBALS`, as T61's `WeakMap`): programs that don't name it
// don't load it. A view keeps a `Uint8Array` over its bytes; values go through typed arrays over an
// 8-byte scratch buffer (Wasm is little-endian, so big-endian accesses reverse the bytes), which also
// do the spec's conversions (ToInt8 … ToUint32, `Float32` rounding). Views on resizable buffers track
// their length (T47's resizable buffers) and a detached or out-of-bounds view throws V8's
// TypeError. Differences from node: called without `new`, the class's own TypeError.
'use strict';

const inspectCustom = Symbol.for('nodejs.util.inspect.custom');

const scratch = new ArrayBuffer(8);
const u8 = new Uint8Array(scratch);
const i8 = new Int8Array(scratch);
const i16 = new Int16Array(scratch);
const u16 = new Uint16Array(scratch);
const i32 = new Int32Array(scratch);
const u32 = new Uint32Array(scratch);
const f32 = new Float32Array(scratch);
const f64 = new Float64Array(scratch);

// ToIndex, with the RangeError `what(value)` describes.
function toIndex(value, what) {
  if (value === undefined) return 0;
  const n = $$toInteger(value);
  if (n < 0 || n > 9007199254740991) throw new RangeError(what(n));
  return n;
}

function outside() {
  return 'Offset is outside the bounds of the DataView';
}

// V8 names a typed array receiver `[object Uint8Array]`.
function describe(v) {
  return __intrinsic_typed_kind(v) >= 0 ? Object.prototype.toString.call(v) : $$describe(v);
}

function incompatible(name, receiver) {
  return new TypeError(`Method DataView.prototype.${name} called on incompatible receiver ${describe(receiver)}`);
}

// IEEE 754 binary16 ↔ number (`getFloat16`/`setFloat16`; js2wasi3 has no `Float16Array`).
function halfToNumber(bits) {
  const sign = bits & 0x8000 ? -1 : 1;
  const e = (bits >> 10) & 0x1f;
  const m = bits & 0x3ff;
  if (e === 0) return sign * m * 2 ** -24;
  if (e === 31) return m === 0 ? sign * Infinity : NaN;
  return sign * (1 + m / 1024) * 2 ** (e - 15);
}

// Rounded to nearest, ties to even.
function numberToHalf(x) {
  if (x !== x) return 0x7e00;
  const sign = x < 0 || (x === 0 && 1 / x < 0) ? 0x8000 : 0;
  const a = Math.abs(x);
  if (a >= 65520) return sign | 0x7c00;
  if (a < 2 ** -14) {
    // Subnormal (or zero): a multiple of 2^-24.
    const m = a * 2 ** 24;
    let f = Math.floor(m);
    const r = m - f;
    if (r > 0.5 || (r === 0.5 && (f & 1) === 1)) f++;
    return sign | f;
  }
  f64[0] = a;
  const hi = u32[1];
  const lo = u32[0];
  const exp = (hi >>> 20) - 1023;
  const mhi = hi & 0xfffff;
  let top = mhi >>> 10;
  // The 42 bits below the half's 10.
  const rest = (mhi & 0x3ff) * 4294967296 + lo;
  const half = 2199023255552;
  if (rest > half || (rest === half && (top & 1) === 1)) top++;
  return sign | (((exp + 15) << 10) + top);
}

// The brand check, for `ArrayBuffer.isView` and `util.types.isDataView`.
let isDataView;

class DataView {
  #buffer;
  #offset;
  #length;
  #bytes;

  constructor(buffer, byteOffset = undefined, byteLength = undefined) {
    if (!__intrinsic_is_array_buffer(buffer)) {
      throw new TypeError('First argument to DataView constructor must be an ArrayBuffer');
    }
    const offset = toIndex(byteOffset, (n) => `Start offset ${n} is outside the bounds of the buffer`);
    const bufferLength = $$bufferState(buffer);
    if (bufferLength < 0) throw new TypeError('Cannot perform DataView constructor on a detached ArrayBuffer');
    const tracking = byteLength === undefined && $$bufferResizable(buffer);
    if (offset > bufferLength) {
      throw new RangeError(`Start offset ${offset} is outside the bounds of the buffer`);
    }
    let length;
    if (byteLength === undefined) {
      length = bufferLength - offset;
    } else {
      length = toIndex(byteLength, (n) => `Invalid DataView length ${n}`);
      if (offset + length > bufferLength) throw new RangeError(`Invalid DataView length ${length}`);
    }
    this.#buffer = buffer;
    this.#offset = offset;
    // -1: a length-tracking view (its `Uint8Array` tracks too).
    this.#length = tracking ? -1 : length;
    this.#bytes = tracking ? new Uint8Array(buffer, offset) : new Uint8Array(buffer, offset, length);
  }

  get buffer() {
    if (!(#bytes in this)) throw new TypeError(`Method get DataView.prototype.buffer called on incompatible receiver ${describe(this)}`);
    return this.#buffer;
  }

  get byteLength() {
    if (!(#bytes in this)) throw new TypeError(`Method get DataView.prototype.byteLength called on incompatible receiver ${describe(this)}`);
    return DataView.#viewLength(this, 'get DataView.prototype.byteLength');
  }

  get byteOffset() {
    if (!(#bytes in this)) throw new TypeError(`Method get DataView.prototype.byteOffset called on incompatible receiver ${describe(this)}`);
    DataView.#viewLength(this, 'get DataView.prototype.byteOffset');
    return this.#offset;
  }

  // GetViewByteLength, after IsViewOutOfBounds (V8's TypeError names `what`).
  static #viewLength(view, what) {
    const size = $$bufferState(view.#buffer);
    const end = view.#length < 0 ? view.#offset : view.#offset + view.#length;
    if (size < 0 || end > size) throw new TypeError(`Cannot perform ${what} on a detached ArrayBuffer`);
    return view.#length < 0 ? size - view.#offset : view.#length;
  }

  // GetViewValue: the `size` bytes at `index` into the scratch buffer, little-endian first.
  static #load(view, name, index, littleEndian, size) {
    if (!(#bytes in view)) throw incompatible(name, view);
    const at = toIndex(index, outside);
    const little = !!littleEndian;
    if (at + size > DataView.#viewLength(view, `DataView.prototype.${name}`)) throw new RangeError(outside());
    const b = view.#bytes;
    for (let i = 0; i < size; i++) u8[little ? i : size - 1 - i] = b[at + i];
  }

  // SetViewValue's checks; the caller converts the value first (ToNumber/ToBigInt), then `#store`.
  static #check(view, name) {
    if (!(#bytes in view)) throw incompatible(name, view);
  }

  static #store(view, name, at, littleEndian, size) {
    const little = !!littleEndian;
    if (at + size > DataView.#viewLength(view, `DataView.prototype.${name}`)) throw new RangeError(outside());
    const b = view.#bytes;
    for (let i = 0; i < size; i++) b[at + i] = u8[little ? i : size - 1 - i];
  }

  getInt8(byteOffset) {
    DataView.#load(this, 'getInt8', byteOffset, true, 1);
    return i8[0];
  }

  setInt8(byteOffset, value) {
    DataView.#check(this, 'setInt8');
    const at = toIndex(byteOffset, outside);
    i8[0] = +value;
    DataView.#store(this, 'setInt8', at, true, 1);
  }

  getUint8(byteOffset) {
    DataView.#load(this, 'getUint8', byteOffset, true, 1);
    return u8[0];
  }

  setUint8(byteOffset, value) {
    DataView.#check(this, 'setUint8');
    const at = toIndex(byteOffset, outside);
    u8[0] = +value;
    DataView.#store(this, 'setUint8', at, true, 1);
  }

  getInt16(byteOffset, littleEndian = undefined) {
    DataView.#load(this, 'getInt16', byteOffset, littleEndian, 2);
    return i16[0];
  }

  setInt16(byteOffset, value, littleEndian = undefined) {
    DataView.#check(this, 'setInt16');
    const at = toIndex(byteOffset, outside);
    i16[0] = +value;
    DataView.#store(this, 'setInt16', at, littleEndian, 2);
  }

  getUint16(byteOffset, littleEndian = undefined) {
    DataView.#load(this, 'getUint16', byteOffset, littleEndian, 2);
    return u16[0];
  }

  setUint16(byteOffset, value, littleEndian = undefined) {
    DataView.#check(this, 'setUint16');
    const at = toIndex(byteOffset, outside);
    u16[0] = +value;
    DataView.#store(this, 'setUint16', at, littleEndian, 2);
  }

  getInt32(byteOffset, littleEndian = undefined) {
    DataView.#load(this, 'getInt32', byteOffset, littleEndian, 4);
    return i32[0];
  }

  setInt32(byteOffset, value, littleEndian = undefined) {
    DataView.#check(this, 'setInt32');
    const at = toIndex(byteOffset, outside);
    i32[0] = +value;
    DataView.#store(this, 'setInt32', at, littleEndian, 4);
  }

  getUint32(byteOffset, littleEndian = undefined) {
    DataView.#load(this, 'getUint32', byteOffset, littleEndian, 4);
    return u32[0];
  }

  setUint32(byteOffset, value, littleEndian = undefined) {
    DataView.#check(this, 'setUint32');
    const at = toIndex(byteOffset, outside);
    u32[0] = +value;
    DataView.#store(this, 'setUint32', at, littleEndian, 4);
  }

  getFloat32(byteOffset, littleEndian = undefined) {
    DataView.#load(this, 'getFloat32', byteOffset, littleEndian, 4);
    return f32[0];
  }

  setFloat32(byteOffset, value, littleEndian = undefined) {
    DataView.#check(this, 'setFloat32');
    const at = toIndex(byteOffset, outside);
    f32[0] = +value;
    DataView.#store(this, 'setFloat32', at, littleEndian, 4);
  }

  getFloat64(byteOffset, littleEndian = undefined) {
    DataView.#load(this, 'getFloat64', byteOffset, littleEndian, 8);
    return f64[0];
  }

  setFloat64(byteOffset, value, littleEndian = undefined) {
    DataView.#check(this, 'setFloat64');
    const at = toIndex(byteOffset, outside);
    f64[0] = +value;
    DataView.#store(this, 'setFloat64', at, littleEndian, 8);
  }

  getBigInt64(byteOffset, littleEndian = undefined) {
    DataView.#load(this, 'getBigInt64', byteOffset, littleEndian, 8);
    return BigInt.asIntN(64, (BigInt(u32[1]) << 32n) + BigInt(u32[0]));
  }

  setBigInt64(byteOffset, value, littleEndian = undefined) {
    DataView.#check(this, 'setBigInt64');
    const at = toIndex(byteOffset, outside);
    const v = BigInt.asUintN(64, value);
    u32[0] = Number(v & 0xffffffffn);
    u32[1] = Number(v >> 32n);
    DataView.#store(this, 'setBigInt64', at, littleEndian, 8);
  }

  getBigUint64(byteOffset, littleEndian = undefined) {
    DataView.#load(this, 'getBigUint64', byteOffset, littleEndian, 8);
    return (BigInt(u32[1]) << 32n) + BigInt(u32[0]);
  }

  setBigUint64(byteOffset, value, littleEndian = undefined) {
    DataView.#check(this, 'setBigUint64');
    const at = toIndex(byteOffset, outside);
    const v = BigInt.asUintN(64, value);
    u32[0] = Number(v & 0xffffffffn);
    u32[1] = Number(v >> 32n);
    DataView.#store(this, 'setBigUint64', at, littleEndian, 8);
  }

  getFloat16(byteOffset, littleEndian = undefined) {
    DataView.#load(this, 'getFloat16', byteOffset, littleEndian, 2);
    return halfToNumber(u16[0]);
  }

  setFloat16(byteOffset, value, littleEndian = undefined) {
    DataView.#check(this, 'setFloat16');
    const at = toIndex(byteOffset, outside);
    u16[0] = numberToHalf(+value);
    DataView.#store(this, 'setFloat16', at, littleEndian, 2);
  }

  static {
    isDataView = (v) => typeof v === 'object' && v !== null && #bytes in v;
  }

  // node's `DataView { [byteLength]: 2, [byteOffset]: 0, [buffer]: ArrayBuffer { … } }`
  // (`Sub [DataView]` for a subclass), laid out on one line when it fits in 80 columns.
  [inspectCustom](depth, options, inspect) {
    if (!(#bytes in this)) return this;
    const name = this.constructor && this.constructor.name;
    const base = name && name !== 'DataView' ? `${name} [DataView]` : 'DataView';
    if (depth < 0) return `[${base}]`;
    const entries = [
      `[byteLength]: ${this.byteLength}`,
      `[byteOffset]: ${this.#offset}`,
      `[buffer]: ${inspect(this.#buffer, { depth: depth === null ? null : depth - 1 })}`,
    ];
    const start = entries.length + base.length + 2 + 10;
    let total = entries.length + start;
    let multiline = false;
    for (const e of entries) {
      total += e.length;
      if (e.includes('\n')) multiline = true;
    }
    if (!multiline && total <= 80) return `${base} { ${entries.join(', ')} }`;
    return `${base} {\n  ${entries.map((e) => e.replaceAll('\n', '\n  ')).join(',\n  ')}\n}`;
  }
}

Object.defineProperty(DataView.prototype, Symbol.toStringTag, {
  value: 'DataView', writable: false, enumerable: false, configurable: true,
});
$$setDataViewCheck(isDataView);

module.exports = { DataView, isDataView };
