// Evaluation order (T52): member compound assignment and array destructuring's IteratorClose.
const log = [];
const key = (name) => ({ toString() { log.push("toString " + name); return name; } });

// `o[k] op= v`: node converts the key for the get and again for the set (the spec's once: test262
// `S11.13.2_A7.*_T4`; node is the oracle); a null base throws before the key and the right side.
const o = { a: 1 };
o[key("a")] += 10;
o[key("a")]++;
console.log(o.a, log.splice(0));
for (const base of [null, undefined]) {
  try {
    base[{ [Symbol.toPrimitive]() { log.push("converted"); } }] ^= (log.push("rhs"), 1);
  } catch (e) {
    console.log(e.constructor.name, e.message, log.splice(0));
  }
  try {
    base[{ [Symbol.toPrimitive]() { throw new Error("key converted"); } }] = 1;
  } catch (e) {
    console.log(e.constructor.name, e.message);
  }
}

// Array destructuring: the target's reference before the step, IteratorClose on abrupt exits.
function counted(values) {
  const c = { next: 0, ret: 0 };
  const iterable = {
    [Symbol.iterator]() {
      let i = 0;
      return {
        next() {
          c.next++;
          return i < values.length ? { value: values[i++], done: false } : { value: undefined, done: true };
        },
        return() {
          c.ret++;
          return {};
        },
      };
    },
  };
  return [iterable, c];
}
let [it, c] = counted([1, 2, 3]);
const t = {};
[t.a, t.b] = it;
console.log(t, c);
[it, c] = counted([1]);
try {
  [{}[(() => { throw new Error("target"); })()]] = it;
} catch (e) {
  console.log(e.message, c);
}
[it, c] = counted([1, 2]);
try {
  [t.x = (() => { throw new Error("default"); })()] = [undefined];
} catch (e) {
  console.log(e.message);
}
function* g() {
  let x;
  [x = yield "paused"] = it;
  return x;
}
const gen = g();
console.log(gen.next(), c);
console.log(gen.return(7), c);
[it, c] = counted([1, 2, 3]);
const rest = {};
[t.first, ...rest.list] = it;
console.log(t.first, rest.list, c);

// Undeclared targets throw; a rest-only object pattern checks for null.
try {
  [notDeclared] = [1];
} catch (e) {
  console.log(e.constructor.name, e.message);
}
let r;
for (const v of [null, undefined]) {
  try {
    ({ ...r } = v);
  } catch (e) {
    console.log(e.constructor.name, e.message);
  }
}
