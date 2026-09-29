// Subclasses of Map, Set, ArrayBuffer and the typed arrays take the subclass's prototype (T47).
class M extends Map {
  getOr(k, d) { return this.has(k) ? this.get(k) : d; }
}
const m = new M([[1, "one"]]);
console.log(m instanceof M, m instanceof Map, m.getOr(1), m.getOr(2, "none"), m.size, m.constructor.name);
class S extends Set {}
const s = new S([1, 2, 2]);
console.log(s instanceof S, s.size, [...s].join());
class Bytes extends Uint8Array {
  sum() { return this.reduce((a, b) => a + b, 0); }
}
const b = new Bytes([1, 2, 3]);
console.log(b instanceof Bytes, b instanceof Uint8Array, b.sum(), b.length, Bytes.BYTES_PER_ELEMENT, b.constructor.name);
class Buf extends ArrayBuffer {}
const buf = new Buf(4);
console.log(buf instanceof Buf, buf.byteLength, new Uint8Array(buf).length);
const r = Reflect.construct(Float32Array, [2], Bytes);
console.log(r instanceof Bytes, Object.getPrototypeOf(r) === Bytes.prototype, r.length);
function Plain() {}
Plain.prototype = null;
console.log(Object.getPrototypeOf(Reflect.construct(Set, [], Plain)) === Set.prototype);
