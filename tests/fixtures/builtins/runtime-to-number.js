// T125 (F85): ToNumber of a string inside the runtime (no conversion in the program's own code): timers' delays,
// array `length`, array-likes' `length`, `defineProperty` of `length`, `bind`. The prelude's StringToNumber is
// always linked now; before, these trapped in a module that never converted a string itself.
"use strict";

const a = [1, 2, 3, 4, 5];
a.length = "3";
const b = [1, 2];
b.length = " 0x4 ";
const c = [1, 2, 3];
Object.defineProperty(c, "length", { value: "1" });
console.log(a, b.length, c);
try { [].length = "1.5"; } catch (e) { console.log(e.constructor.name, e.message); }
try { [].length = "abc"; } catch (e) { console.log(e.constructor.name, e.message); }

function count() { return arguments.length; }
console.log(count.apply(null, { length: "2" }), count.apply(null, { length: "1e1" }), Reflect.apply(count, null, { length: " 3 " }));
function two(x, y) {}
const bound = two.bind(null);
console.log(bound.length, two.bind(null, 1).length);

const order = [];
setTimeout(() => order.push("20"), "20");
setTimeout(() => order.push("0x5"), "0x5");
setTimeout(() => order.push("junk"), "junk");
setTimeout(() => order.push("1e1"), "1e1");
const id = setTimeout(() => order.push("cleared"), "15");
clearTimeout(id);
let n = 0;
const iv = setInterval(() => { if (++n === 2) { clearInterval(iv); } }, "3");
setTimeout(() => console.log(order.join(" "), n), 60);
