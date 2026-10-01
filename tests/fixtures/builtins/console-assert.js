// console.assert (T61): node writes "Assertion failed" lines to stderr.
console.assert(true,"x"); console.assert(false); console.assert(0, "bad %s", "thing", 2); console.assert(null, {a:1}); console.log(console.assert(false,"r"))
