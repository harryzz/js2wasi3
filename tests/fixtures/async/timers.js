// Timers: order by deadline then creation, microtasks between callbacks, arguments, `this`,
// clearing (also from inside callbacks), intervals, the delay clamp and async code around them.
// Each phase waits for the previous one; deadlines that could tie are far apart (node's timer
// start times depend on how long its own code takes).
const log = (...a) => console.log(a.join(" "));
const sleep = (ms, v) => new Promise((r) => setTimeout(r, ms, v));

async function order() {
  setTimeout(() => log("40"), 40);
  setTimeout(() => log("20 a"), 20);
  setTimeout(() => log("20 b"), 20);
  setTimeout(() => log("1"), 1);
  await sleep(60);
}

// A timer made by a callback waits for the next pass, after every timer due now (here only
// ones with the same deadline, so how late the pass runs doesn't matter).
async function nested() {
  setTimeout(() => {
    log("A");
    Promise.resolve().then(() => log("A micro"));
    queueMicrotask(() => log("A queueMicrotask"));
    setTimeout(() => log("A + 0"), 0);
  }, 1);
  setTimeout(() => log("B"), 1);
  await sleep(30);
}

async function args() {
  const t = setTimeout(function (a, b, c) {
    log("args", a, b, c.length, typeof this, this === t);
  }, 1, 1, "two", [3]);
  for (const bad of [-5, NaN, undefined, 0, null, 1.5]) setTimeout(() => log("clamped", `${bad}`), bad);
  const never = setTimeout(() => log("never"), 60000);
  clearTimeout(never);
  clearTimeout(undefined);
  clearTimeout(42);
  clearTimeout({});
  const cancelLater = setTimeout(() => log("cancelled later"), 30);
  setTimeout(() => { log("clearing"); clearTimeout(cancelLater); }, 10);
  try { setTimeout("not a function", 1); } catch (e) { log(e.name, e.message); }
  try { setInterval(); } catch (e) { log(e.name, e.message); }
  log(typeof setTimeout, setTimeout.name, clearInterval.name, typeof t);
  await sleep(50);
}

async function intervals() {
  let ticks = 0;
  await new Promise((done) => {
    const iv = setInterval(() => {
      ticks++;
      log("tick", ticks);
      if (ticks === 4) { clearInterval(iv); done(); }
    }, 10);
    const once = setInterval(() => { log("once"); clearInterval(once); }, 1);
  });
}

async function combinators() {
  log("sleep", await sleep(20, "a"));
  log("race", await Promise.race([sleep(40, "slow"), sleep(5, "fast")]));
  log("all", (await Promise.all([sleep(5, 1), sleep(15, 2), 3])).join(","));
  for (let i = 0; i < 3; i++) await sleep(1);
  log("loop done");
}

await order();
await nested();
await args();
await intervals();
await combinators();
log("end");
