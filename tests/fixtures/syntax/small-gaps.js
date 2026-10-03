// T63: a rest parameter beside `arguments`, `arguments` in a class constructor, `delete a?.[b]`,
// a `var` read before its declaration.

// winston's `Logger.log(level, msg, ...splat)`.
class Logger {
  log(level, msg, ...splat) {
    return [arguments.length, level, msg, splat, Array.isArray(splat)];
  }
}
const lg = new Logger();
console.log(lg.log("info", "hi", 1, 2), lg.log("warn"), lg.log("a", "b"));
function f(a, { b } = {}, ...[c, d]) {
  arguments[0] = "changed";
  return [a, b, c, d, arguments.length, arguments[0]];
}
console.log(f(1, { b: 2 }, 3, 4, 5), f());
const g = function (...xs) {
  xs.push("x");
  return [xs, arguments.length, [...arguments]];
};
console.log(g(1, 2));
function outer(first, ...more) {
  const inner = () => [arguments.length, first, more];
  return inner();
}
console.log(outer(7, 8, 9));

// argparse: `arguments` in a class constructor.
class Action {
  constructor(options) {
    this.n = arguments.length;
    this.options = options;
    this.args = Array.prototype.slice.call(arguments);
  }
}
const a = new Action({ x: 1 }, "extra");
console.log(a.n, a.options, a.args);
class Sub extends Action {
  constructor() {
    super(...arguments);
    this.own = arguments.length;
  }
}
const s = new Sub("p", "q", "r");
console.log(s.n, s.own, s.options, s.args);

// zigbee2mqtt's settings: `delete settings.devices?.[id]`.
const settings = { devices: { a: 1, b: 2 } };
console.log(delete settings.devices?.["a"], settings.devices);
const none = {};
console.log(delete none.devices?.["a"], delete none?.x, delete undefined?.y, none);
let n = 0;
const counted = () => (n++, "b");
console.log(delete settings?.devices[counted()], n, settings);
console.log(delete null?.[counted()], n);
const frozen = Object.freeze({ k: 1 });
try {
  delete frozen?.k;
} catch (e) {
  console.log(e instanceof TypeError);
}

// babel's regenerator: a `var` read in the loop condition before its `var` in the body.
function search(r, list) {
  for (var t = 0; !o && t < list.length; t++) {
    var o = list[t] === r;
  }
  return [o, t];
}
console.log(search(2, [1, 2, 3]), search(9, [1]), search(0, []));
function counter(n) {
  while (typeof total === "undefined" || total < n) {
    var total = (total || 0) + 1;
  }
  return total;
}
console.log(counter(3));
