// T64: DataView.
import util from "node:util";
const b = new ArrayBuffer(16);
const d = new DataView(b, 2, 12);
console.log(d.byteLength, d.byteOffset, d.buffer === b, ArrayBuffer.isView(d), DataView.length, DataView.name);
console.log(Object.prototype.toString.call(d), String(d), Object.getOwnPropertyNames(DataView.prototype).join());
console.log(d.getFloat64.length, d.setFloat64.length, d.getInt8.length, d.setInt8.length);

d.setInt8(0, -1);
d.setUint8(1, 300);
d.setInt16(2, -2);
d.setInt16(4, 0x1234, true);
console.log(d.getInt8(0), d.getUint8(0), d.getUint8(1), d.getInt16(2), d.getUint16(2), d.getUint16(4), d.getUint16(4, true));
d.setUint32(0, 0xdeadbeef);
d.setInt32(4, -123456789, true);
console.log(d.getUint32(0), d.getInt32(0), d.getUint32(0, true), d.getInt32(4, true), d.getInt32(4));
d.setFloat32(0, Math.PI);
d.setFloat64(4, -Math.E, true);
console.log(d.getFloat32(0), d.getFloat64(4, true), d.getFloat64(4), d.getFloat32(0, true));
d.setBigInt64(0, -5n);
d.setBigUint64(4, 2n ** 64n - 3n, true);
console.log(d.getBigInt64(0), d.getBigUint64(0), d.getBigUint64(4, true), d.getBigInt64(4, true));
for (const x of [1, -2.5, 65504, 65520, 1e-8, 5.960464477539063e-8, 0.1, -0, NaN, Infinity, 1 / 3, 2049, 2051]) {
  d.setFloat16(0, x);
  d.setFloat16(2, x, true);
  console.log(x, d.getUint16(0), d.getFloat16(0), d.getFloat16(2, true));
}
console.log(new Uint8Array(b));
d.setUint8(0, "7");
d.setInt16(0, { valueOf: () => 65537 });
console.log(d.getInt16(0), d.getUint8(1));

for (const f of [
  () => new DataView(b, 17),
  () => new DataView(b, -1),
  () => new DataView(b, 4, 13),
  () => new DataView({}),
  () => new DataView(new Uint8Array(4)),
  () => d.getUint32(9),
  () => d.getInt8(-1),
  () => d.setFloat64(5, 1),
  () => d.setBigInt64(0, 1),
  () => DataView.prototype.getInt8.call({}, 0),
  () => DataView.prototype.setInt8.call(new Uint8Array(2), 0, 1),
]) {
  try {
    f();
    console.log("no error");
  } catch (e) {
    console.log(e.constructor.name, e.message);
  }
}

const whole = new DataView(b);
const tail = new DataView(b, 16);
console.log(whole.byteLength, tail.byteLength, new DataView(b, 3).byteOffset);
console.log(d);
console.log(new DataView(new ArrayBuffer(2)));
console.log([tail]);
class Sub extends DataView {
  first() {
    return this.getUint8(0);
  }
}
const s = new Sub(b, 1, 2);
console.log(s instanceof DataView, s.first(), s.byteLength, ArrayBuffer.isView(s), ArrayBuffer.isView({}));
console.log(Object.getOwnPropertyDescriptor(DataView.prototype, Symbol.toStringTag));

console.log(util.types.isDataView(d), util.types.isDataView(new Uint8Array(1)), util.types.isArrayBufferView(d));
