// An exception in a queueMicrotask callback is uncaught; later jobs don't run.
queueMicrotask(() => console.log("first"));
queueMicrotask(() => {
  throw new RangeError("boom");
});
queueMicrotask(() => console.log("not reached"));
Promise.resolve().then(() => console.log("not reached either"));
console.log("sync");
