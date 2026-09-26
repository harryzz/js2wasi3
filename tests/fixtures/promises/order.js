// Ordering of promise jobs and microtasks (node is the oracle).
console.log("start");
queueMicrotask(() => console.log("microtask 1"));
const p = new Promise((resolve) => {
  console.log("executor");
  resolve(1);
});
p.then((v) => {
  console.log("then 1", v);
  return v + 1;
}).then((v) => console.log("then 2", v));
Promise.resolve("a").then((v) => console.log("resolved", v));
Promise.reject(new Error("no")).catch((e) => console.log("caught", e.message));
queueMicrotask(() => console.log("microtask 2"));
console.log("end");
