// for await: node's microtask ticks per step for sync and async iterables, early exits, errors (T20 slice 1a).
const log = [];
const tick = (name, n) => { let p = Promise.resolve(); for (let i = 1; i <= n; i++) { const k = i; p = p.then(() => log.push(name + k)); } return p; };
async function a() { for await (const x of [1, 2]) log.push("a" + x); log.push("a end"); }
async function b() {
  const it = { i: 0, [Symbol.asyncIterator]() { return this; }, next() { this.i++; return Promise.resolve({ value: this.i, done: this.i > 2 }); } };
  for await (const x of it) log.push("b" + x); log.push("b end");
}
async function c() { for await (const x of [Promise.resolve(1), 2]) { log.push("c" + x); break; } log.push("c end"); }
function* g() { try { yield 1; yield 2; } finally { log.push("g finally"); } }
async function d() { for await (const x of g()) { log.push("d" + x); break; } log.push("d end"); }
async function e() { try { for await (const x of [1, Promise.reject("boom"), 3]) log.push("e" + x); } catch (err) { log.push("e caught " + err); } }
function* g2() { try { yield 1; yield Promise.reject("r2"); yield 3; } finally { log.push("g2 finally"); } }
async function f() { try { for await (const x of g2()) log.push("f" + x); } catch (err) { log.push("f caught " + err); } }
const run = async (fn) => { log.length = 0; const p = fn(); await tick("t", 12); await p; console.log(fn.name, log.join(" ")); };
await run(a); await run(b); await run(c); await run(d); await run(e); await run(f);
