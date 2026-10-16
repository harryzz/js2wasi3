// T127 (F438): Promise subclasses: `then`/`catch`/`finally` make the species (NewPromiseCapability, the spec's job
// order also for a constructor that wraps its executor), the statics take `this`, `Symbol.species` overrides, the
// TypeErrors, `await`, and `console.log`'s `P [Promise] { … }`.
"use strict";
{
class P extends Promise {}
const r = [];
r.push(P.resolve(1) instanceof P, P.reject(2).catch(() => {}) instanceof P, P.all([1]) instanceof P, P.race([1]) instanceof P, P.allSettled([1]) instanceof P, P.any([1]) instanceof P, P.withResolvers().promise instanceof P);
const p = new P(res => res(1));
r.push(p.then(x => x) instanceof P, p.catch(() => {}) instanceof P, p.finally(() => {}) instanceof P);
console.log(r.join(" "));
}
{
const log = [];
class P extends Promise {}
const p = P.resolve(1);
const q = Promise.resolve(2);
p.then(v => log.push("p1 " + v)).then(() => log.push("p2"));
q.then(v => log.push("q1 " + v)).then(() => log.push("q2"));
p.then(() => log.push("p3"));
class Back extends Promise { static get [Symbol.species]() { return Promise; } }
const b = Back.resolve(3).then(x => x);
log.push("back " + (b instanceof Back) + " " + (b instanceof Promise));
let constructed = 0;
class Wrap extends Promise {
  constructor(executor) {
    constructed++;
    super((res, rej) => executor(v => { log.push("wrap resolve"); res(v); }, rej));
  }
}
const w = Wrap.resolve(4).then(v => v * 10);
w.then(v => log.push("wrap " + v + " " + (w instanceof Wrap)));
class Bad1 extends Promise { constructor(ex) { super(ex); ex(() => {}, () => {}); } }
try { new Bad1(() => {}).then(); log.push("bad1 none"); } catch (e) { log.push("bad1 " + e.constructor.name); }
const odd = Promise.resolve(5);
odd.constructor = 1;
try { odd.then(); } catch (e) { log.push("ctor " + e.message); }
const odd2 = Promise.resolve(6);
odd2.constructor = { [Symbol.species]: 7 };
try { odd2.then(); } catch (e) { log.push("species " + e.message); }
const f = P.resolve(8).finally(() => {});
f.then(v => log.push("finally " + v + " " + (f instanceof P)));
P.reject(new Error("no")).catch(e => log.push("caught " + e.message));
(async () => { const v = await P.resolve(9); log.push("await " + v); })();
console.log(new P(() => {}), P.resolve(1));
setTimeout(() => console.log(log.join(" | "), constructed), 10);
}
