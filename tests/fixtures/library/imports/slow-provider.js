// Implements interface `acme:demo/slow`: `async func`s that wait on timers (`tick` returns at once).
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export const slow = {
  async fetch(n) {
    await sleep(50);
    return new Uint8Array(n).fill(7);
  },
  async delayEcho(s) {
    await sleep(5);
    if (s === "") throw "empty";
    return s + s;
  },
  async tick() {},
};
