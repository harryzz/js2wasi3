// T141: a local that only holds arrays keeps its receiver typed: element reads and writes on it.
function basic(n) {
  const a = [n], b = [];
  for (let i = 0; i < n; i++) a[i] = i * 2;
  for (let i = 0; i < n; i++) b[i] = a[n - 1 - i] + 1;
  let s = 0;
  for (let i = 0; i < n; i++) { const t = a[i]; a[i] = b[i]; b[i] = t; s += a[i] - b[i]; }
  return [a.join(), b.join(), s, a.length, b[n + 3], a[-1], a[1.5], a["1"]];
}
console.log(basic(6));

// Reassigned to other arrays; the receiver is read before the key runs.
function reassign() {
  let a = [1, 2, 3];
  const first = a[(a = [10, 20, 30], 1)];
  const second = a[(a = [100, 200, 300], 2)];
  a[(a = [7, 8, 9], 0)] = 5;
  return [first, second, a.join()];
}
console.log(reassign());

// A `var` read before its array is stored (undefined), and one stored in a branch.
function vars(flag) {
  var a;
  try { a[0]; } catch (e) { console.log(e.name); }
  if (flag) a = [1, 2];
  try { a[0] = 5; } catch (e) { console.log(e.name); }
  return a === undefined ? "undef" : a.join();
}
console.log(vars(true), vars(false));

// Assigned an array or a non-array: stays generic.
function mixed(flag) {
  let a = [1, 2, 3];
  if (flag) a = "str";
  if (flag === 2) a = { 0: "obj", length: 1 };
  const out = [];
  for (let i = 0; i < 3; i++) out.push(a[i]);
  try { a[0] = 9; } catch (e) { out.push(e.name); }
  return out.join() + "|" + (typeof a === "string" ? a : a[0]);
}
console.log(mixed(0), mixed(1), mixed(2));

// Aliases: a copy of an array local, a parameter, a call result.
function alias(p) {
  const a = [1, 2, 3];
  const b = a;
  b[1] = 20;
  let c = p;
  c[0] = "p";
  let d = a;
  d = p;
  d[2] = "q";
  return [a.join(), b.join(), p.join(), c.join(), d.join()];
}
console.log(alias([0, 0, 0]));

// A spread literal, holes, and writes past the end or into holes.
function shapes() {
  const src = [1, 2], a = [...src, 3], h = [1, , 3];
  a[5] = 6; h[1] = 2; h[4] = 5;
  return [a.length, a.join(), h.join(), 1 in [1, , 3], Object.keys(a).join()];
}
console.log(shapes());

// Frozen, sealed, non-extensible arrays; read-only length.
function attrs() {
  "use strict";
  const f = Object.freeze([1, 2]), s = Object.seal([1, 2]), n = Object.preventExtensions([1, 2]);
  const r = [];
  for (const [name, a] of [["f", f], ["s", s], ["n", n]]) {
    try { a[0] = 9; a[5] = 1; r.push(name + ":ok"); } catch (e) { r.push(name + ":" + e.name); }
  }
  const t = [1, 2, 3];
  Object.defineProperty(t, "length", { writable: false });
  try { t[5] = 1; } catch (e) { r.push("t:" + e.name); }
  return r.concat([f[0], s[0], n[0], t.length]);
}
console.log(attrs());

// Captured by a closure (a cell, not typed), and closures that write it.
function captured() {
  const a = [1, 2, 3];
  const set = (i, v) => { a[i] = v; };
  set(1, 20);
  let b = [4, 5];
  const swap = () => { b = [b[1], b[0]]; };
  swap();
  b[0] += 1;
  return [a.join(), b.join()];
}
console.log(captured());

// Loops creating a new array every iteration; destructuring; array-likes.
function loops() {
  const out = [];
  for (let i = 0; i < 3; i++) {
    const row = [];
    for (let j = 0; j <= i; j++) row[j] = i * 10 + j;
    out.push(row);
  }
  const [p, q = [9]] = [[1], undefined];
  p[1] = 2; q[1] = 3;
  const typed = new Uint8Array(3), buf = [typed][0];
  buf[1] = 300; buf[5] = 1;
  return [JSON.stringify(out), p.join(), q.join(), buf.join(), buf[5]];
}
console.log(loops());

// Number mode, NaN, -0, big indexes, numeric strings, symbols, getters on Array.prototype.
function numbers() {
  const a = [0.5, 1.5], b = [1, 2, 3];
  a[1] = NaN; a[2] = -0; b[0] = 0.25; b["2"] = 7; b[-1] = "neg";
  Array.prototype[7] = "proto";
  const r = [a.length, Object.is(a[2], -0), a[1], b.join().slice(0, 12), b[-1], [][7], b[7]];
  delete Array.prototype[7];
  return r;
}
console.log(numbers());

// Evaluation order: receiver, key, value.
function order() {
  const log = [], a = [0, 0, 0];
  const k = (n) => { log.push("k" + n); return n; };
  const v = (n) => { log.push("v" + n); return n; };
  a[k(1)] = v(5);
  a[k(2)] += v(1);
  const got = a[k(0)];
  return [log.join(), a.join(), got];
}
console.log(order());

// Generators and async functions keep their array locals across suspensions.
function* gen() { const a = [1]; yield a[0]; a[0] = 2; yield a[0]; a[1] = 3; yield a.join(); }
console.log([...gen()]);
(async () => { const a = [1, 2]; await null; a[0] = 5; await null; console.log("async", a[0] + a[1]); })();
