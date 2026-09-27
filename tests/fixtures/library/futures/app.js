// Reads the futures `acme:futures/later` returns, and writes one it takes.
import { doubleLater, shout, origin, done } from "acme:futures/later";

export async function go(n) {
  const order = [];
  const r = await doubleLater(n);
  order.push(`now ${r.now} ${r.later instanceof Promise}`);
  const p = r.later.then((v) => {
    order.push(`later ${v}`);
    return v;
  });
  // Written after the app's timer, read by the provider.
  const s = new Promise((resolve) => setTimeout(() => resolve("hey"), 5));
  order.push(await shout(s));
  order.push(`awaited ${await p}`);
  const pt = await origin();
  order.push(`point ${pt.x},${pt.y}`);
  order.push(`done ${await done()}`);
  return order.join("; ");
}
