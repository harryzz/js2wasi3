// Logical assignment `&&=`, `||=`, `??=`: short-circuit (no evaluation, no write), results,
// member targets evaluated once, private fields, function names, const.
const log = (...a) => console.log(a.join(" "));

let a = 0, b = 1, c = null, d = undefined, e = "", f = "x", g = false;
log(a ||= 5, b ||= 5, c ??= "c", d ??= "d", e &&= "no", f &&= "yes", g ||= true);
log(a, b, c, d, e, f, g);

let n = 3;
n &&= n * 2;
n ||= 100;
n ??= 200;
log("n", n);

let calls = 0;
const side = (v) => { calls++; return v; };
let t = 1, z = 0, u;
t ||= side("no");
z &&= side("no");
t ??= side("no");
log("calls after short-circuits", calls);
u ??= side("yes");
z ||= side(7);
log("calls", calls, u, z);

// Members: getters/setters show what's read and written; keys and objects evaluated once.
const events = [];
const obj = {
  _v: 0,
  get v() { events.push("get"); return this._v; },
  set v(x) { events.push(`set ${x}`); this._v = x; },
};
obj.v ||= 1;
obj.v ||= 2;
obj.v &&= 3;
obj.v ??= 4;
log(events.join(","), obj._v);

let keyCalls = 0;
const key = () => { keyCalls++; return "k"; };
let objCalls = 0;
const store = { k: null };
const target = () => { objCalls++; return store; };
target()[key()] ??= "filled";
target()[key()] ??= "again";
log(keyCalls, objCalls, store.k);

const arr = [0, 1, null];
arr[0] ||= "zero";
arr[1] &&= "one";
arr[2] ??= "two";
arr[5] ??= "five";
log(arr.length, arr[0], arr[1], arr[2], arr[5]);

// Results of the expressions themselves.
const o2 = { p: null, q: 0 };
log(o2.p ??= 1, o2.q ||= 2, o2.q &&= 3, o2.x ??= undefined);

// Private fields.
class Box {
  #v = null;
  fill(x) { return this.#v ??= x; }
  bump() { return this.#v &&= this.#v + 1; }
  get v() { return this.#v; }
}
const box = new Box();
log(box.fill(10), box.fill(20), box.bump(), box.v);

// Anonymous functions get the variable's name.
let fn;
fn ??= function () {};
let arrow;
arrow ||= () => 1;
log(fn.name, arrow.name);

// const: only an actual write throws.
const k = 1;
k ||= 2;
log("const kept", k);
try {
  k &&= 2;
} catch (err) {
  log(err.name, err.message);
}

// Inside conditions and loops.
let count = null;
for (let i = 0; i < 3; i++) {
  if ((count ??= 0) >= 0) count += i;
}
log("count", count);
