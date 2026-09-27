// for await: errors from the iterable and its iterator, `return()` on early exit (T20 slice 1a; split out of
// for-await-order.js for F157).
try { for await (const x of 5) {} } catch (err) { console.log(err.message); }
try { for await (const x of { [Symbol.asyncIterator]() { return { next() { return 1; } }; } }) {} } catch (err) { console.log(err.message); }
try { for await (const x of { [Symbol.asyncIterator]: 5 }) {} } catch (err) { console.log(err.message); }
const it2 = { [Symbol.asyncIterator]() { return { next: () => ({ value: 1, done: false }), return() { console.log("return called"); return {}; } }; } };
for await (const x of it2) { console.log("x", x); break; }
const it3 = { [Symbol.asyncIterator]() { return { next: () => ({ value: 1, done: false }), return() { return 5; } }; } };
try { for await (const x of it3) break; } catch (err) { console.log(err.message); }
