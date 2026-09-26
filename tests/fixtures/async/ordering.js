// Interleaving of async functions, promise reactions and queueMicrotask (node is the oracle).
const log = (...a) => console.log(a.join(" "));

async function a1() {
  log("a1 start");
  await a2();
  log("a1 end");
}
async function a2() {
  log("a2");
}
log("script start");
setTimeoutless();
a1();
new Promise((resolve) => {
  log("promise executor");
  resolve();
}).then(() => log("promise then 1")).then(() => log("promise then 2"));
queueMicrotask(() => log("microtask"));
log("script end");

function setTimeoutless() {}

// Awaiting non-promises, resolved promises, thenables and pending promises.
async function kinds() {
  log("k0", await 1);
  log("k1", await Promise.resolve(2));
  log("k2", await { then(r) { r(3); } });
  let resolve;
  const p = new Promise((r) => { resolve = r; });
  queueMicrotask(() => resolve(4));
  log("k3", await p);
  return "kinds done";
}
kinds().then((v) => log(v));

// Returning a promise takes two extra ticks.
async function retPromise() { return Promise.resolve("rp"); }
async function retValue() { return "rv"; }
retPromise().then((v) => log("returned", v));
retValue().then((v) => log("returned", v));
Promise.resolve().then(() => log("t1")).then(() => log("t2")).then(() => log("t3")).then(() => log("t4"));

// Two async functions interleave at each await.
async function ping(name, n) {
  for (let i = 0; i < n; i++) {
    log(name, i);
    await null;
  }
  return name;
}
Promise.all([ping("ping", 3), ping("pong", 3)]).then((r) => log("all", r.join(",")));
