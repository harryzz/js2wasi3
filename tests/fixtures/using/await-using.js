// `await using` (T45): async disposal with the spec's awaits (tick order checked against other jobs).
const log = [];
const ares = (name, fail) => ({
  async [Symbol.asyncDispose]() { log.push("adispose " + name); if (fail) throw new Error(name); },
});
const sres = (name) => ({ [Symbol.dispose]() { log.push("dispose " + name); } });

async function basic() {
  {
    await using a = ares("a");
    using s = sres("s");
    await using b = sres("b-sync-fallback");
    await using n = null;
    log.push("body");
  }
  log.push("after");
}

async function ticks() {
  Promise.resolve().then(() => log.push("tick 1")).then(() => log.push("tick 2")).then(() => log.push("tick 3"));
  { await using n = null; log.push("block"); }
  log.push("after null");
}

async function errors() {
  try {
    await using a = ares("A", true), b = ares("B", true);
    throw new Error("body");
  } catch (e) {
    log.push(`${e.name}: ${e.error.message} / ${e.suppressed.name}: ${e.suppressed.error.message} / ${e.suppressed.suppressed.message}`);
  }
  try { await using x = {}; } catch (e) { log.push(e.name + ": " + e.message); }
  try { await using x = 1; } catch (e) { log.push(e.name + ": " + e.message); }
}

async function loops() {
  for (await using r of [ares("of1"), ares("of2")]) log.push("iter");
  for await (using r of [sres("fa1")]) log.push("for await");
}

await basic();
console.log(log.splice(0));
await ticks();
console.log(log.splice(0));
await errors();
console.log(log.splice(0));
await loops();
console.log(log.splice(0));
{
  await using top = ares("top");
  log.push("top body");
}
console.log(log.splice(0));
