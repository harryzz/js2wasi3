// T99: constant-key reads that find the key on a prototype use a per-site cache (receiver's prototype, holder,
// entry, epoch). Every way of changing what `o.m` means must still be seen at the same site.
"use strict";
const call = (o) => o.m(); // one site for everything below
const read = (o) => o.p;
const out = [];
const log = (...a) => out.push(a.join(" "));

class A { m() { return "A.m"; } get p() { return "A.p"; } }
class B extends A {}
class C extends B {}
const a = new A(), b = new B(), c = new C();
for (let i = 0; i < 3; i++) log("plain", call(a), call(b), call(c), read(c));

// Replace the method on the holder (same key, new value).
A.prototype.m = function () { return "A.m2"; };
log("replaced", call(a), call(b), call(c));

// Shadow it on an intermediate prototype, then on one instance.
B.prototype.m = function () { return "B.m"; };
log("shadow B", call(a), call(b), call(c));
const c2 = new C();
c2.m = () => "own";
log("own", call(c), call(c2));
delete B.prototype.m;
log("unshadowed", call(b), call(c), call(c2));

// Delete from the holder: the next level (Object.prototype) has no `m` until it is added there.
delete A.prototype.m;
try { call(c); } catch (e) { log("deleted", e.constructor.name); }
Object.prototype.m = function () { return "Object.m"; };
log("object proto", call(a), call(c));
delete Object.prototype.m;
A.prototype.m = function () { return "A.m3"; };
log("restored", call(c));

// The receiver's prototype changes; an intermediate prototype's prototype changes.
const d = new C();
log("before", call(d));
Object.setPrototypeOf(d, { m() { return "other"; } });
log("receiver proto", call(d), call(c));
const X = { m() { return "X.m"; } };
Object.setPrototypeOf(B.prototype, X);
log("chain changed", call(b), call(c));
Object.setPrototypeOf(B.prototype, A.prototype);
log("chain back", call(b), call(c));

// A getter becomes a data property and back.
log("getter", read(c));
Object.defineProperty(A.prototype, "p", { value: "data p", configurable: true, writable: true });
log("data", read(c));
Object.defineProperty(A.prototype, "p", { get() { return "getter again:" + (this === c); }, configurable: true });
log("getter again", read(c));

// Receivers of different classes alternating at one site.
class P { m() { return "P"; } }
class Q { m() { return "Q"; } }
const rs = [new P(), new Q(), new P(), b, new Q()];
log("mixed", rs.map(call).join(","));
let n = 0;
for (let i = 0; i < 1000; i++) n += call(rs[i % 5]).length;
log("loop", n);
console.log(out.join("\n"));
