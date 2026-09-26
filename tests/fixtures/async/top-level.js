// Top-level await: the module body is an async function; other jobs run while it waits.
const log = (...a) => console.log(a.join(" "));
let state = "start";
Promise.resolve().then(() => { log("job sees", state); });
log("before await");
const v = await new Promise((r) => queueMicrotask(() => r(7)));
state = "resumed";
log("after await", v);
async function twice(x) { return 2 * (await x); }
let total = 0;
for (let i = 1; i <= 3; i++) total += await twice(i);
log("total", total);
try {
  await Promise.reject(new SyntaxError("top"));
} catch (e) {
  log("caught at top level:", e.name, e.message);
}
function later() { return state; }
log("later()", later());
