// Awaits the `async func`s of `acme:demo/slow`, several at once, with a timer of its own.
import { fetch, delayEcho, tick } from "acme:demo/slow";

export async function go(n) {
  const order = [];
  await tick(); // the provider is idle: the subtask returns at once
  order.push("first");
  setTimeout(() => order.push("timer"), 1); // the app's own timers run while imports are pending
  const a = fetch(n).then((b) => {
    order.push("fetch");
    return b;
  });
  const b = delayEcho("hi");
  await tick();
  order.push("tick");
  const bytes = await a;
  let e;
  try {
    await delayEcho("");
  } catch (x) {
    e = `${x instanceof Error} ${x.message}/${x.payload}`;
  }
  return `${[...bytes]} ${await b} ${e} ${order}`;
}
