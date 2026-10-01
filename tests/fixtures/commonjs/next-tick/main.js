// After a CommonJS entry's code node runs the `process.nextTick` queue before the microtasks
// (after an ES module's, the microtasks first) (T60).
process.nextTick(() => {
  console.log("t1");
  process.nextTick(() => console.log("t2"));
  Promise.resolve().then(() => console.log("m2"));
});
Promise.resolve().then(() => {
  console.log("m1");
  process.nextTick(() => console.log("t3"));
});
console.log("main", typeof require);
