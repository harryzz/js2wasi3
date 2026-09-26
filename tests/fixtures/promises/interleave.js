// A classic ordering puzzle: nested then, queueMicrotask inside handlers, resolve with thenables.
const out = [];
const log = (s) => out.push(s);

Promise.resolve()
  .then(() => {
    log("A1");
    queueMicrotask(() => log("A1-mt"));
    return Promise.resolve("x");
  })
  .then(() => log("A2"));

Promise.resolve()
  .then(() => log("B1"))
  .then(() => log("B2"))
  .then(() => log("B3"))
  .then(() => log("B4"))
  .then(() => log("B5"));

new Promise((res) => {
  log("C0");
  res({ then: (f) => { log("C-then"); f("c"); } });
}).then((v) => log("C1 " + v));

queueMicrotask(() => {
  log("M1");
  Promise.resolve().then(() => log("M1-then"));
});

const rejected = Promise.reject("r");
rejected.then(null, () => log("R1"));
rejected.catch(() => log("R2")).finally(() => log("R3"));

let n = 0;
function tick() {
  if (++n < 5) {
    log("tick " + n);
    queueMicrotask(tick);
  }
}
queueMicrotask(tick);

Promise.all([Promise.resolve(1), 2]).then(() => log("ALL"));
Promise.race([new Promise(() => {}), Promise.resolve()]).then(() => log("RACE"));

// print once everything settled: this runs after all the jobs above
const done = () => console.log(out.join("\n"));
let wait = Promise.resolve();
for (let i = 0; i < 20; i++) wait = wait.then(() => {});
wait.then(done);
