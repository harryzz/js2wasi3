// Array writes to absent elements go up the prototype chain (T34, F162): a setter there is
// called with the array as `this`, a read-only property there makes the write throw. The log is
// a string (writes to a log array would meet the same prototype properties); `JSON.stringify`'s
// internal arrays must not (T34: the prelude defines, it doesn't [[Set]]).
let out = "";
const log = (...xs) => {
  out += xs.map((x) => JSON.stringify(x)).join(" ") + "\n";
};
const attempt = (f) => {
  try {
    log(f());
  } catch (e) {
    log(e.constructor.name, e.message);
  }
};

let target;
Object.defineProperty(Array.prototype, "1", {
  get() {
    return "proto-get";
  },
  set(v) {
    log("proto-set", v, this === target);
  },
  configurable: true,
});
target = [0];
attempt(() => {
  target[1] = "x";
  return [target.length, target.hasOwnProperty(1)];
});
attempt(() => {
  target.push("pushed");
  return [target.length, target.hasOwnProperty(1)];
});
const holey = [0, , 2];
target = holey;
attempt(() => {
  holey[1] = "y";
  return holey.hasOwnProperty(1);
});
// An own element is written directly.
const own = [0, 1];
target = own;
attempt(() => {
  own[1] = "own";
  return own[1];
});
delete Array.prototype[1];

// Read-only on the chain: the write throws, nothing is defined.
Object.defineProperty(Object.prototype, "0", { value: "ro", writable: false, configurable: true });
const empty = [];
attempt(() => {
  empty[0] = 1;
});
attempt(() => empty.push(1));
attempt(() => [empty.length, empty.hasOwnProperty(0), empty[0]]);
delete Object.prototype[0];
attempt(() => {
  empty.push(1);
  return empty;
});

// A getter without a setter on the chain.
Object.defineProperty(Array.prototype, "0", { get: () => "g", configurable: true });
attempt(() => {
  const a = [];
  a[0] = 1;
});
delete Array.prototype[0];

// Another prototype through `__proto__`.
const proto = {
  set 2(v) {
    log("custom proto set", v);
  },
};
const custom = [1, 2];
custom.__proto__ = proto;
attempt(() => {
  custom[2] = 3;
  return [custom.length, Object.keys(custom)];
});

// Without prototype index keys nothing changes.
const plain = [1];
plain[3] = 4;
attempt(() => [plain.length, Object.keys(plain)]);
console.log(out.trimEnd());
