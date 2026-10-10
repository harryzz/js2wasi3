// T103: `var x;` written before every read stays a number; a read that can come first sees `undefined`.
function plain(n) {
  var a, b, s;
  a = 1.5; b = 2; s = 0;
  for (var i = 0; i < n; i++) s += a * b + i;
  return s;
}
function branch(c) {
  var x, y;
  if (c) { x = 1; y = 2; } else { x = 3; }
  return [x, typeof y, y];
}
function loopSecond(n) {
  var prev, out = [];
  for (var i = 0; i < n; i++) {
    out.push(prev);
    prev = i * 0.5;
  }
  return out;
}
function afterLoop(n) {
  var last;
  while (n-- > 0) last = n;
  return last;
}
function afterBreak(xs) {
  var found;
  for (var k = 0; k < xs.length; k++) {
    if (xs[k] > 2) break;
    found = xs[k];
  }
  return found;
}
function inTry(f) {
  var r;
  try { r = f(); } catch (e) { return 'caught ' + r; } finally { console.log('finally', r); }
  return r;
}
function typeofFirst() {
  var t;
  var before = typeof t;
  t = 4;
  return before + ' ' + t;
}
function closure() {
  var c;
  var read = () => c;
  var r0 = read();
  c = 5;
  return [r0, read()];
}
function redeclared(n) {
  var s = 0;
  for (var j = 0; j < n; j++) { var v; if (j % 2) v = j; s += v === undefined ? 100 : v; }
  return s;
}
function forIn(o) {
  var k, keys = '';
  for (k in o) keys += k;
  return keys + ' ' + k;
}
function labeled(c) {
  var z;
  out: { if (c) break out; z = 7; }
  return z;
}
function conditional(c) {
  var p, q;
  c ? (p = 1) : (p = 2);
  c && (q = 3);
  return [p, q];
}
function switched(k) {
  var w;
  switch (k) { case 1: w = 'one'; break; case 2: w = 2; }
  return w;
}
function doWhile() {
  var d, n = 0;
  do { d = n * 1.25; n++; } while (n < 3);
  return d;
}
function returnsEarly(c) {
  var m;
  if (!c) return 'none';
  else m = 9.5;
  return m * 2;
}
console.log(plain(10), branch(true), branch(false));
console.log(loopSecond(3), afterLoop(3), afterLoop(0), afterBreak([1, 2, 3]), afterBreak([5]));
console.log(inTry(() => 1.25), inTry(() => { throw 1; }));
console.log(typeofFirst(), closure(), redeclared(5), forIn({ a: 1, b: 2 }), forIn({}));
console.log(labeled(true), labeled(false), conditional(true), conditional(false));
console.log(switched(1), switched(2), switched(3), doWhile(), returnsEarly(true), returnsEarly(false));
