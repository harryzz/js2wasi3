// for await: top level, patterns, labels, return/throw/break closing, errors from return (T20 slice 1a).
for await (const ch of "ab") console.log("top", ch);
for await (const [k, v] of new Map([["x", 1], ["y", Promise.resolve(2)]])) console.log(k, v);
let last;
for await (last of [3, 4]);
console.log("last", last);
async function find(xs, want) {
  for await (const x of xs) if (x === want) return "found " + x;
  return "none";
}
console.log(await find([1, Promise.resolve(2), 3], 2), await find([], 1));
outer: for await (const a of [1, 2, 3]) {
  for await (const b of [10, 20]) {
    if (b === 20) continue outer;
    if (a === 3) break outer;
    console.log(a, b);
  }
}
const tracked = {
  [Symbol.asyncIterator]() {
    let i = 0;
    return {
      next: async () => ({ value: i++, done: i > 5 }),
      return: async (v) => { console.log("return", v); return { done: true }; },
    };
  },
};
try {
  for await (const x of tracked) { if (x === 2) throw new Error("stop at " + x); }
} catch (e) { console.log(e.message); }
for await (const x of tracked) { if (x === 1) break; }
const bad = { [Symbol.asyncIterator]() { return { next: async () => ({ value: 1, done: false }), return: () => { throw new Error("return threw"); } }; } };
try { for await (const x of bad) throw new Error("body threw"); } catch (e) { console.log(e.message); }
try { for await (const x of bad) break; } catch (e) { console.log(e.message); }
const rej = { [Symbol.asyncIterator]() { return { next: () => Promise.reject(new Error("next rejected")), return() { console.log("never"); } }; } };
try { for await (const x of rej); } catch (e) { console.log(e.message); }
