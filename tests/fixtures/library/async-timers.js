// T20 slice 3: an async export waits for timers; they run while the call waits.
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
let log = "";
setTimeout(() => {
  log += "init timer;";
}, 0);

/** @returns {Promise<string>} */
export async function waits() {
  await sleep(20);
  log += "slept;";
  setImmediate(() => {
    log += "immediate;";
  });
  process.nextTick(() => {
    log += "tick;";
  });
  await sleep(1);
  return log;
}
export async function interval() {
  let n = 0;
  await new Promise((resolve) => {
    const id = setInterval(() => {
      if (++n === 3) {
        clearInterval(id);
        resolve();
      }
    }, 2);
  });
  return n;
}
export async function timerThrows() {
  setTimeout(() => {
    throw new TypeError("in a timer");
  }, 1);
  await sleep(10);
  return 1;
}
export async function rejectsLater() {
  await sleep(1);
  throw new RangeError("later");
}
/** @returns {Promise<number>} */
export async function race() {
  return Promise.race([sleep(50).then(() => 1), sleep(5).then(() => 2)]);
}
