// The global object (T42): built-ins as its properties, undeclared names through it.
console.log(typeof globalThis, globalThis === global, globalThis.globalThis === globalThis);
console.log(globalThis.Array === Array, globalThis.Math === Math, global.parseInt === parseInt);
console.log(globalThis.Map === Map, globalThis.Error === Error, globalThis.Promise === Promise);
console.log(globalThis.setTimeout === setTimeout, globalThis.JSON === JSON);

const d = Object.getOwnPropertyDescriptor(globalThis, "Array");
console.log(d.writable, d.enumerable, d.configurable, d.value === Array);
const n = Object.getOwnPropertyDescriptor(globalThis, "NaN");
console.log(n.writable, n.enumerable, n.configurable, n.value);
const g = Object.getOwnPropertyDescriptor(globalThis, "globalThis");
console.log(g.writable, g.enumerable, g.configurable);
console.log(Object.getOwnPropertyDescriptor(globalThis, "global").enumerable);

// Undeclared names read and write the global object.
console.log(typeof counter, "counter" in globalThis);
try { counter; } catch (e) { console.log(e.name, e.message); }
try { counter = 1; } catch (e) { console.log(e.name, e.message); }
globalThis.counter = 10;
console.log(counter, typeof counter);
counter++;
counter += 5;
++counter;
console.log(counter, globalThis.counter);
counter = "text";
console.log(counter, globalThis.counter);
function bump() { return ++counter2; }
global.counter2 = 1;
console.log(bump(), bump(), counter2);
delete globalThis.counter;
try { counter; } catch (e) { console.log(e.name, e.message); }
console.log(typeof counter);

// Read-only globals.
try { NaN = 1; } catch (e) { console.log(e.name, e.message); }
try { undefined = 1; } catch (e) { console.log(e.name, e.message); }
try { globalThis.Infinity = 1; } catch (e) { console.log(e.name, e.message); }
console.log(Infinity, globalThis.undefined);

// Accessors on the global object.
Object.defineProperty(globalThis, "answer", { get() { return 42; }, configurable: true });
console.log(answer, typeof answer);
