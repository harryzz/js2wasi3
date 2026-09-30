// node's `util` (T55): format, inspect (depth), inherits, promisify, callbackify, deprecate,
// types, isDeepStrictEqual, debuglog, errors.
import util, { format, inspect, promisify, inherits, types, isDeepStrictEqual, callbackify, deprecate } from "node:util";
import { isDate } from "util/types";
console.log(format("%s=%d %j %i%%", "a", 42, { x: [1] }, 7.5), format(1, "b", { c: 2 }), format());
const deep = { a: { b: { c: { d: { e: 1 } } } }, s: "str" };
console.log(inspect(deep), inspect(deep, { depth: 0 }), inspect(deep, { depth: 1 }));

console.log(inspect("q'uote"), inspect([[1, [2, [3, [4]]]]], { depth: 1 }), inspect({ a: 1 }, false, 0));
function A() {} A.prototype.hi = function () { return "hi"; };
function B() { A.call(this); }
inherits(B, A);
console.log(new B().hi(), B.super_ === A, Object.getOwnPropertyDescriptor(B, "super_"));
const cbFn = (x, cb) => setTimeout(() => (x < 0 ? cb(new Error("neg")) : cb(null, x * 2)), 1);
const p = promisify(cbFn);
console.log(await p(21), p.name, p.length);
try { await p(-1); } catch (e) { console.log("rejected", e.message); }
const custom = function () {}; custom[promisify.custom] = () => "custom!";
console.log(promisify(custom)(), promisify.custom === Symbol.for("nodejs.util.promisify.custom"));
const cbd = callbackify(async (x) => x + 1);
cbd(1, (err, v) => console.log("callbackified", err, v, cbd.name, cbd.length));
const cbr = callbackify(async () => { throw null; });
cbr((err) => console.log("falsy", err.code, err.message, err.reason));
const old = deprecate(function old(a) { return a * 3; }, "old() is deprecated", "DEP0999");
process.on("warning", (w) => console.log("warning", w.name, w.message, w.code));
console.log(old(2), old(3));
console.log(types.isDate(new Date()), isDate({}), types.isRegExp(/x/), types.isPromise(Promise.resolve()), types.isMap(new Map()), types.isSet(new Set()),
  types.isAsyncFunction(async () => {}), types.isGeneratorFunction(function* () {}), types.isTypedArray(new Uint8Array(2)), types.isUint8Array(new Uint8Array(1)),
  types.isBoxedPrimitive(Object(1)), types.isNumberObject(1), types.isNativeError(new TypeError()), types.isDate.name);
console.log(isDeepStrictEqual({ a: [1, 2, { b: 3 }] }, { a: [1, 2, { b: 3 }] }), isDeepStrictEqual({ a: 1 }, { a: "1" }), isDeepStrictEqual([0], [-0]),
  isDeepStrictEqual(NaN, NaN), isDeepStrictEqual(new Map([[1, { a: 1 }]]), new Map([[1, { a: 1 }]])), isDeepStrictEqual(new Set([{ x: 1 }]), new Set([{ x: 1 }])),
  isDeepStrictEqual(new Date(5), new Date(5)), isDeepStrictEqual(new Date(5), new Date(6)), isDeepStrictEqual(/a/g, /a/g), isDeepStrictEqual(/a/g, /a/i),
  isDeepStrictEqual([1, , 3], [1, undefined, 3]), isDeepStrictEqual(Object.create(null), {}));
console.log(util.isArray([]), util.stripVTControlCharacters("\u001b[31mred\u001b[39m"), util.toUSVString("a\ud800b"), util.inspect.custom);
const dbg = util.debuglog("foo"); dbg("hidden %d", 1); console.log(dbg.enabled);
try { promisify(1); } catch (e) { console.log(e.code, e.message); }
