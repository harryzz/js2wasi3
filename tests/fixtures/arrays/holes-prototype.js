// Holes read through the prototype chain in the native array paths (T33, F159): `join`,
// `toString`, `pop`, spread, `for-of`, destructuring, `Array.from`. Outputs are collected while
// `Array.prototype` has index properties and printed after they are removed (node's own
// `console.log` fails with an index getter on `Array.prototype`). The log is a string: writes
// to an array would meet the prototype's setter (F162).
let out = "";
const show = (x) => {
  out += JSON.stringify(x) + "\n";
};

let a;
Object.defineProperty(Array.prototype, "1", {
  get() {
    return "proto:" + (this === a);
  },
  set(v) {},
  configurable: true,
});
a = [0, , 2];
show(a[1]);
show(a.join());
show(String(a));
show([...a]);
for (const x of a) show(x);
const [, second] = a;
show(second);
show(Array.from(a));
show(a.map((x) => x));
const b = [0, ,];
show(b.pop());
show(b.length);
show(1 in a);
show(a.hasOwnProperty(1));
delete Array.prototype[1];

Array.prototype[3] = "three";
const c = [1, 2, 3, , 5];
show(c.join("|"));
show([...c]);
show(c.pop());
show(c.pop());
show(c.length);
delete Array.prototype[3];

// Without prototype elements holes are `undefined` again.
show([1, , 3].join());
show([...[1, , 3]]);
console.log(out.trimEnd());
console.log([1, , 3]);
