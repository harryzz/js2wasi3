// T137: `f.apply(x, arguments)` forwards the argument list when `f.apply` is the built-in; otherwise the
// `arguments` object is made (once) and passed.

// Prototype.js's `Class.create`, as in octane's raytrace.
var Class = {
  create: function () {
    return function () {
      this.initialize.apply(this, arguments);
    };
  },
};
var Point = Class.create();
Point.prototype.initialize = function (x, y, z) {
  this.x = x;
  this.y = y;
  this.n = arguments.length;
  this.z = z;
};
var p = new Point(1, "two");
console.log(p.x, p.y, p.z, p.n);
console.log(new Point().n, new Point(1, 2, 3, 4).n);

// The callee sees its own `arguments`, not the caller's, and can't change the caller's.
function wrap() {
  var r = inner.apply(null, arguments);
  return [r, wrap2.apply(null, arguments)];
}
function inner(a) {
  arguments[0] = "changed";
  a = "also";
  return arguments.length;
}
function wrap2() {
  return Array.prototype.slice.call(arguments);
}
console.log(wrap(1, 2, 3));
console.log(wrap());

// `this` as the first argument, strict and sloppy.
function strictThis() {
  "use strict";
  return this;
}
function sloppyThis() {
  return typeof this;
}
function fwd(f) {
  return f.apply(undefined, arguments);
}
console.log(fwd(strictThis), fwd(sloppyThis));
function fwdThis(f, t) {
  return f.apply(t, arguments);
}
console.log(fwdThis(strictThis, 7), fwdThis(sloppyThis, 7));

// An own `apply`, a replaced `Function.prototype.apply`: they get the `arguments` object, one per call.
function viaApply(f) {
  return f.apply(f, arguments);
}
var seen = [];
var own = function () {};
own.apply = function (t, args) {
  seen.push(args);
  return typeof args + " " + args.length + " " + args[1];
};
console.log(viaApply(own, "b"), seen[0] === seen[0], Array.isArray(seen[0]));
function twice(f) {
  f.apply(null, arguments);
  return f.apply(null, arguments);
}
var got = [];
var rec = function () {};
rec.apply = function (t, args) {
  got.push(args);
  return got.length;
};
console.log(twice(rec, 1), got[0] === got[1], got[0].length);

// Not a function: node's TypeError.
function notFn(x) {
  return x.apply(null, arguments);
}
try {
  notFn({ apply: Function.prototype.apply });
} catch (e) {
  console.log(e instanceof TypeError);
}
try {
  notFn(5);
} catch (e) {
  console.log(e instanceof TypeError);
}

// The built-in taken from elsewhere: a bound function, `apply` on a class (throws), on a built-in.
function viaBound() {
  return Math.max.apply(Math, arguments);
}
console.log(viaBound(3, 9, 4), viaBound());
class K {}
function callClass() {
  return K.apply(null, arguments);
}
try {
  callClass(1);
} catch (e) {
  console.log(e instanceof TypeError);
}

// `arguments` used another way too: made up front as before.
function mixed(f) {
  var n = arguments.length;
  return [n, f.apply(null, arguments)];
}
console.log(mixed(function () { return arguments.length; }, 1, 2));

// An arrow's `arguments` is the enclosing function's.
function arrowFwd(f) {
  var g = () => f.apply(null, arguments);
  return g();
}
console.log(arrowFwd(function (a, b) { return [a, b]; }, "x"));

// Evaluation order: the receiver, `apply`, then the first argument.
function order(o) {
  var log = [];
  return [
    (log.push("recv"), o).apply((log.push("this"), null), arguments),
    log,
  ];
}
var traced = function () { return arguments.length; };
console.log(order(traced, 1));
