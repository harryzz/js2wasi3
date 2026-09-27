// Array.prototype.concat with Symbol.isConcatSpreadable and array-likes (T35 slice 2).
const t = (f) => { try { console.log(f()); } catch (e) { console.log(e.constructor.name + ": " + e.message); } };
const spreadObj = { length: 3, 0: "a", 2: "c", [Symbol.isConcatSpreadable]: true };
t(() => [1].concat(spreadObj));
const noSpread = [4, 5]; noSpread[Symbol.isConcatSpreadable] = false;
t(() => [1].concat(noSpread).length);
t(() => [1].concat({ length: 2 ** 53 - 1, [Symbol.isConcatSpreadable]: true }));
t(() => Array.prototype.concat.call("ab", "cd"));
t(() => [].concat.call(1, 2).map((x) => typeof x));
t(() => [1, , 3].concat([, 5]));
t(() => [].concat({ [Symbol.isConcatSpreadable]: 0, length: 1, 0: "x" }));
t(() => [].concat({ [Symbol.isConcatSpreadable]: "yes", length: "2", 0: "x", 1: "y" }));
