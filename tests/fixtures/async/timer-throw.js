// An exception in a timer callback ends the program; later timers don't run.
setTimeout(() => console.log("never runs"), 50);
setTimeout(() => {
  console.log("first");
  queueMicrotask(() => console.log("jobs queued before the throw still don't run"));
  throw new Error("from a timer");
}, 5);
console.log("scheduled");
