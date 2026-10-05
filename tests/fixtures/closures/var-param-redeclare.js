// T88: `var` redeclaring a parameter keeps the argument (plain, captured, destructured, rest, default),
// and `var x;` in a loop or at the top level resets nothing.
function f(a, b) {
  var a;
  var b;
  return [a, b];
}
console.log(f(1, 2));
const g = function (x, y) {
  var t;
  var x;
  var y;
  while (true) { if (x > 3) return [x, y]; x++; }
};
console.log(g(1, "y"));
function h(p) { var p = p + 1; return p; }
console.log(h(5));
function loop2() {
  const out = [];
  for (let i = 0; i < 3; i++) {
    var seen;
    out.push(seen);
    seen = i;
  }
  return out;
}
console.log(loop2());
var top1 = 5;
var top1;
console.log(top1);
function c(a) {
  var a;
  const get = () => a;
  return get();
}
console.log(c(7));
function d(a, b) {
  var b;
  var a = a * 2;
  return [a, b, arguments.length, (() => b)()];
}
console.log(d(3, "b"));
function e({ x }, [y]) { var x; var y; return x + y; }
console.log(e({ x: 1 }, [2]));
function r(...rest) { var rest; return rest.length; }
console.log(r(1, 2, 3));
function df(a = 10) { var a; return a; }
console.log(df(), df(4));
