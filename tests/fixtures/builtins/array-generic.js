// Array.prototype push/pop/join/toString on array-likes and primitives (T35 slice 1): the prelude's
// generic algorithms behind the native array fast path.
const t = (f) => { try { console.log(f()); } catch (e) { console.log(e.constructor.name + ": " + e.message); } };
t(() => Array.prototype.push.call(null, 1));
t(() => Array.prototype.pop.call(undefined));
t(() => Array.prototype.join.call(null));
t(() => Array.prototype.toString.call(null));
t(() => Array.prototype.push.call({ length: 2 ** 53 - 1 }, 1));
t(() => Array.prototype.toString.call({ join: 1 }));
t(() => Array.prototype.toString.call({ join() { return "J"; } }));
const o = { length: 2, 0: "a", 1: "b" };
t(() => [Array.prototype.push.call(o, "c", "d"), JSON.stringify(o)]);
t(() => [Array.prototype.pop.call(o), JSON.stringify(o)]);
t(() => Array.prototype.join.call(o, "-"));
t(() => Array.prototype.join.call("abc"));
t(() => Array.prototype.pop.call({}));
t(() => Array.prototype.push.call(Object.freeze({ length: 0 }), 1));
