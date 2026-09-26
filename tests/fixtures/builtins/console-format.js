// T25 slice 5: console.log format strings (F23): %s %d %i %f %j %O %c %% (node's formatWithOptions).
// (%o of an object traps: showHidden isn't implemented.)

console.log("%s is %d years", "Bob", 42, "extra", { a: 1 });
console.log("%i %f %d %d", "42.9px", "3.5e2", -0, Symbol("x"));
console.log("%j %j %s", { a: [1, "x"] }, undefined, null);
console.log("%O and %o", { deep: { er: { est: { x: 1 } } } }, "str");
console.log("%s", { nested: { obj: { x: 1 } }, arr: [1, [2]] }, [1, 2]);
class P { toString() { return "P!"; } } console.log("%s|%s", new P(), [new P()]);
console.log("100%% sure %c styled", "color: red", "after");
console.log("%x unknown %", 1);
console.log("only %s", "one", "two");
console.log("%s");
console.log("no args %s");
const fmt = "dyn %d"; console.log(fmt, 5);
const o = {}; o.o = o; console.log("%j", o);
console.log(5, "%s", "x");
console.log("%s", -0, "%d", true);
