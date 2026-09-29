// Resizable ArrayBuffers, length-tracking and out-of-bounds typed arrays (T47).
const t = (f) => { try { console.log(f()); } catch (e) { console.log(e.constructor.name + ": " + e.message); } };
const rab = new ArrayBuffer(4, { maxByteLength: 8 });
console.log(rab, rab.resizable, rab.maxByteLength, rab.byteLength);
const fx = new ArrayBuffer(4);
console.log(fx.resizable, fx.maxByteLength);
t(() => new ArrayBuffer(8, { maxByteLength: 4 }));
t(() => new ArrayBuffer(4, { maxByteLength: -1 }));
t(() => new ArrayBuffer(4, { maxByteLength: 2 ** 33 }).maxByteLength);
t(() => new ArrayBuffer(4, { maxByteLength: 2 ** 53 }).maxByteLength);
t(() => new ArrayBuffer(4, { maxByteLength: undefined }).resizable);
t(() => new ArrayBuffer(4, 5).resizable);
t(() => new ArrayBuffer(1, { maxByteLength: "3" }).maxByteLength);
t(() => new ArrayBuffer(1, { get maxByteLength() { throw new Error("getter"); } }));
t(() => rab.resize(9));
t(() => rab.resize(-1));
t(() => fx.resize(2));
t(() => ArrayBuffer.prototype.resize.call({}, 2));
t(() => Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "resizable").get.call({}));
t(() => ArrayBuffer.length);

// Views: length-tracking (no length), fixed-length, tracking with an offset.
const lt = new Uint8Array(rab);
const fl = new Uint8Array(rab, 0, 4);
const lo = new Uint16Array(rab, 2);
lt[0] = 1;
lt[3] = 4;
console.log(lt, fl, lo);
rab.resize(8);
console.log(lt, fl, lo, lt.length, lo.byteLength, lt.byteLength);
lt[7] = 7;
console.log(lt[7], lo[2], lt.at(-1), [...lt].join());
rab.resize(2);
console.log(lt, fl, lo, fl.length, fl.byteLength, fl.byteOffset, lo.byteOffset, fl[0], lo.length);
fl[0] = 9;
console.log(lt[0], 0 in fl, 1 in lt, 2 in lt, Object.keys(lt), Object.keys(fl));
t(() => fl.fill(1));
t(() => fl.at(0));
t(() => fl.join());
t(() => fl.toString());
t(() => fl.findLastIndex((x) => x));
t(() => fl.values());
t(() => [...fl]);
t(() => lo.join());
t(() => fl.subarray(0).length);
rab.resize(6);
console.log(lt, fl, lo, rab);
t(() => new Uint16Array(rab, 1));
t(() => new Uint16Array(rab, 0, 4));
t(() => new Uint16Array(rab, 8));
t(() => new Uint32Array(rab));
t(() => new Uint8Array(rab, 6).length);

// Methods on length-tracking views follow the buffer.
const nums = new Int16Array(rab);
nums.set([5, -3, 7]);
console.log(nums.map((x) => x * 2), nums.filter((x) => x > 0), nums.slice(1), nums.toSorted(), nums.indexOf(7));
console.log(nums.reduce((a, b) => a + b), Array.from(nums), new Int16Array(nums), Int16Array.from(nums));
rab.resize(8);
console.log(nums.length, nums.includes(0), nums.with(3, 42), nums.toReversed());

// subarray of a tracking view without `end` tracks too.
const sub = lt.subarray(1);
const sub2 = lt.subarray(1, 3);
rab.resize(5);
console.log(sub.length, sub2.length, sub, sub2);

// Resizing while iterating.
const it = lt.values();
console.log(it.next().value);
rab.resize(1);
console.log(it.next());
rab.resize(8);
const fixedIt = fl.values();
rab.resize(2);
t(() => fixedIt.next());
rab.resize(8);
let seen = [];
for (const x of lt) {
  seen.push(x);
  if (seen.length === 2) rab.resize(3);
}
console.log(seen);
console.log({ rab }, [new ArrayBuffer(2, { maxByteLength: 3 })]);
