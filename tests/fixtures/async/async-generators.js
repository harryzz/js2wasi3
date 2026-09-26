// Async generators: request queue order and ticks, return/throw at start and after completion, finally on break, printing (T20 slice 1b).
const log = [];
const tick = (name, n) => { let p = Promise.resolve(); for (let i = 1; i <= n; i++) { const k = i; p = p.then(() => log.push(name + k)); } return p; };
const run = async (name, fn) => { log.length = 0; const p = fn(); await tick("t", 10); await p; console.log(name, log.join(" ")); };
async function* g1() { log.push("start"); yield 1; yield Promise.resolve(2); const x = await 3; log.push("x" + x); return 4; }
await run("next x4", async () => { const g = g1(); for (let i = 0; i < 4; i++) g.next().then((r) => log.push(JSON.stringify(r))); });
await run("for await", async () => { for await (const v of g1()) log.push("v" + v); });
await run("return early", async () => { const g = g1(); log.push(JSON.stringify(await g.next())); log.push(JSON.stringify(await g.return(9))); log.push(JSON.stringify(await g.next())); });
await run("return at start", async () => { const g = g1(); g.return(Promise.resolve(7)).then((r) => log.push(JSON.stringify(r))); g.next().then((r) => log.push(JSON.stringify(r))); });
await run("throw at start", async () => { const g = g1(); g.throw(new Error("e")).catch((e) => log.push("caught " + e.message)); g.next().then((r) => log.push(JSON.stringify(r))); });
async function* g2() { try { yield 1; yield 2; } finally { log.push("finally"); await null; log.push("after await in finally"); } }
await run("break", async () => { for await (const v of g2()) { log.push("v" + v); break; } log.push("after loop"); });
async function* g3() { try { const r = yield 1; log.push("got " + r); } catch (e) { log.push("gen caught " + e); yield "recovered"; } }
await run("throw into", async () => { const g = g3(); await g.next(); log.push(JSON.stringify(await g.throw("boom"))); log.push(JSON.stringify(await g.next())); });
await run("next value", async () => { const g = g3(); await g.next(); log.push(JSON.stringify(await g.next("hello"))); });
async function* g4() { yield Promise.reject("rej"); }
await run("yield rejected", async () => { const g = g4(); try { await g.next(); } catch (e) { log.push("caught " + e); } log.push(JSON.stringify(await g.next())); });
async function* g5() { throw new Error("body"); }
await run("throws", async () => { const g = g5(); g.next().catch((e) => log.push("caught " + e.message)); g.next().then((r) => log.push(JSON.stringify(r))); });
const ag = g1();
console.log(typeof ag.next, typeof ag[Symbol.asyncIterator], ag[Symbol.asyncIterator]() === ag, Object.prototype.toString.call(ag), String(g1));
console.log(ag, g1);
const obj = { async *m() { yield this === obj; } };
for await (const v of obj.m()) console.log("method", v);
class C { static async *s(n) { for (let i = 0; i < n; i++) yield i; } }
const out = []; for await (const v of C.s(3)) out.push(v); console.log(out);
const f = async function* () { yield 1; yield 2; };
try { for await (const v of f()) console.log("delegate", v); } catch (e) { console.log(e.message); }
