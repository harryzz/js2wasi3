// T104 (F276): a `var` with an initializer that a read can reach first is `undefined` there.
function f(c) { if (c) { var x = 1 } return x }
function g(c) { if (c) { var s = 'a' + c } return s }
function h(n) { for (var i = 0; i < n; i++) { var last = i * 1.5 } return last }
function p(c) { if (c) { var [a, b] = [1, 2] } return [a, b] }
function o(c) { if (c) { var { u, v = 'v' } = { u: 3 } } return [u, v] }
function w(c) { while (c) { var t = 'x'; break } return t }
function d() { var k = 0; do { var q = k++ } while (q < 2); return q }
function dj() { var k = 0; do { if (k > 5) break; var r = k++ } while (r < 2); return r }
function before() { var seen = typeof y; var y = 2.5; return seen + ' ' + y }
function tried(fail) { try { if (fail) throw 0; var z = 'ok' } catch { } return z }
function sw(k) { switch (k) { case 1: var m = 1.5; break; default: } return m }
function loop2(n) { var out = []; for (var j = 0; j < n; j++) { out.push(acc); var acc = j } return out }
function arith(c) { if (c) { var n1 = 2 } return n1 + 1 }
console.log(f(false), f(true), g(false), g(1), h(0), h(3), p(false), p(true), o(false), o(true));
console.log(w(false), w(true), d(), dj(), before(), tried(true), tried(false), sw(1), sw(2), loop2(3));
console.log(arith(false), arith(true));
if (process.argv.length > 99) { var top = 5 }
console.log(top);
