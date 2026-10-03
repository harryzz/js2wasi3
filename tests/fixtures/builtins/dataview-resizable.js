// DataView over resizable and detached buffers (T64): length tracking, V8's TypeErrors.
const rab = new ArrayBuffer(8, { maxByteLength: 16 });
const t = new DataView(rab, 2), f = new DataView(rab, 2, 4);
console.log(t.byteLength, f.byteLength); rab.resize(12); console.log(t.byteLength, f.byteLength, t.getInt8(9));
rab.resize(4);
for (const [n, fn] of [["t.byteLength", () => t.byteLength], ["f.byteLength", () => f.byteLength], ["f.byteOffset", () => f.byteOffset], ["f.getInt8", () => f.getInt8(0)], ["t.getInt8", () => t.getInt8(0)], ["t.getInt8 2", () => t.getInt8(2)], ["t.byteLength", () => t.byteLength]]) {
  try { console.log(n, fn()); } catch (e) { console.log(n, e.name, e.message); }
}
rab.resize(1);
for (const [n, fn] of [["t.byteLength", () => t.byteLength], ["t.byteOffset", () => t.byteOffset], ["t.getInt8", () => t.getInt8(0)], ["new", () => new DataView(rab, 2)]]) {
  try { console.log(n, fn()); } catch (e) { console.log(n, e.name, e.message); }
}
const ab = new ArrayBuffer(8); const d = new DataView(ab, 1, 2); ab.transfer();
for (const [n, fn] of [["d.byteLength", () => d.byteLength], ["d.byteOffset", () => d.byteOffset], ["d.getInt8", () => d.getInt8(0)], ["d.setInt8", () => d.setInt8(0, 1)], ["d.buffer", () => d.buffer.byteLength], ["new", () => new DataView(ab)]]) {
  try { console.log(n, fn()); } catch (e) { console.log(n, e.name, e.message); }
}
