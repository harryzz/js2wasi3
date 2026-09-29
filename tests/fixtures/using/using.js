// `using` declarations (T45): disposal at scope exit, in reverse order, with SuppressedError chains.
const log = [];
const res = (name, fail) => ({ name, [Symbol.dispose]() { log.push("dispose " + name); if (fail) throw new Error(name); } });

{
  using a = res("a"), b = res("b");
  using c = null;
  log.push("body");
}
console.log(log.splice(0));

function early(flag) {
  using r = res("r");
  if (flag) return "returned";
  log.push("end");
  return "fell through";
}
console.log(early(true), early(false), log.splice(0));

for (const i of [1, 2]) {
  using x = res("loop" + i);
  if (i === 1) continue;
  log.push("after continue");
}
console.log(log.splice(0));

for (using it of [res("of1"), res("of2")]) log.push("iter " + it.name);
console.log(log.splice(0));

for (using f = res("for"); ; ) break;
console.log(log.splice(0));

try {
  using a = res("A", true), b = res("B", true);
  throw new Error("body");
} catch (e) {
  console.log(e.name, e.message, e.error.message, e.suppressed.name, e.suppressed.error.message, e.suppressed.suppressed.message);
}
log.splice(0);

for (const v of [1, {}, { [Symbol.dispose]: 1 }]) {
  try { using x = v; } catch (e) { console.log(e.name + ": " + e.message); }
}

switch (1) {
  case 1: {
    using s = res("switch");
    log.push("case");
  }
}
console.log(log.splice(0));

function* gen() { using g = res("gen"); yield 1; yield 2; }
for (const v of gen()) break;
console.log(log.splice(0));

Function.prototype[Symbol.dispose] = function () {};
const named = () => { using fn = function () {}; return fn.name; };
console.log(named(), log.length);
class K { static { using s = res("static"); } }
console.log(log.splice(0));
