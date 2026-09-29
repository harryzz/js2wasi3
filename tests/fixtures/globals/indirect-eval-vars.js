// `var` and function declarations in indirect eval code are global-object properties (T44).
(0, eval)("var x = 1; function f() { return x + 1; }");
console.log(x, f(), typeof f, Object.getOwnPropertyDescriptor(globalThis, "x"));
console.log(Object.getOwnPropertyDescriptor(globalThis, "f").enumerable, f.name);
x = 10;
console.log(f(), globalThis.x);

// Hoisting: functions are there before the code runs; vars start `undefined`.
console.log((0, eval)("typeof later + ' ' + String(v); var v = 2; function later() {}"), v);

// Declarations in blocks and loops, `for (var …;;)`, several declarators.
(0, eval)("if (true) { var inBlock = 'b'; } for (var i = 0, n = 3; i < n; i++) {} var a = 1, b, c = a + 1;");
console.log(inBlock, i, n, a, b, c);

// Completion values don't change.
console.log((0, eval)("var y = 5; y"), (0, eval)("1; var z = 2;"), (0, eval)("3; function g() {}"));

// An existing global: `var` keeps it, a function replaces it; `var NaN` is fine.
globalThis.keep = "kept";
(0, eval)("var keep;");
console.log(keep);
(0, eval)("var NaN;");
try { (0, eval)("function NaN() {}"); } catch (e) { console.log(e.name, e.message); }

// Deletable (configurable), and other indirect forms.
console.log(delete globalThis.a, typeof a);
globalThis.eval("var viaMember = 'm'");
eval?.("function viaOptional() { return 'o'; }");
console.log(viaMember, viaOptional());

// Strict eval code keeps its declarations.
console.log((0, eval)("'use strict'; var local = 1; function lf() {} typeof lf"), typeof local, typeof lf);

// A non-extensible global object.
Object.preventExtensions(globalThis);
try { (0, eval)("var fresh;"); } catch (e) { console.log(e.name, e.message); }
try { (0, eval)("function fresh2() {}"); } catch (e) { console.log(e.name, e.message); }
