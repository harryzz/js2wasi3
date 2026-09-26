// A rejected top-level await ends the program like an uncaught exception.
console.log("before");
await null;
console.log("between");
await Promise.reject(new RangeError("top-level rejection"));
console.log("not reached");
