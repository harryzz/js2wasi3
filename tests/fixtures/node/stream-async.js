// node's stream module (T60): async iteration, Readable.from, stream/promises, operators, duplexPair.
import { Readable, Writable, Transform, PassThrough, duplexPair, finished } from "node:stream";
import { pipeline } from "node:stream/promises";
import * as sp from "stream/promises";

async function* gen() {
  yield "a";
  await null;
  yield "b";
  yield "c";
}

async function main() {
  // for await over a Readable.
  const got = [];
  for await (const chunk of Readable.from(gen())) got.push(chunk);
  console.log("for await", got);

  // Bytes with setEncoding, read by async iteration.
  const r = new Readable({ read() {} });
  r.setEncoding("utf8");
  r.push(Buffer.from("héllo "));
  r.push(Buffer.from("wörld"));
  r.push(null);
  let text = "";
  for await (const s of r) text += s;
  console.log("text", JSON.stringify(text), r.readableEncoding);

  // Breaking out of the loop destroys the stream.
  const r2 = Readable.from([1, 2, 3, 4]);
  for await (const x of r2) {
    if (x === 2) break;
  }
  console.log("break", r2.destroyed);

  // stream/promises pipeline with an async generator transform and a writable.
  const out = [];
  await pipeline(
    Readable.from(["x", "y"]),
    async function* (source) {
      for await (const c of source) yield c.toUpperCase() + c;
    },
    new Writable({
      objectMode: true,
      write(c, e, cb) {
        out.push(c);
        cb();
      },
    }),
  );
  console.log("pipeline", out, sp.pipeline === pipeline);

  // A failing pipeline rejects.
  try {
    await pipeline(
      Readable.from(["1"]),
      new Transform({
        transform(c, e, cb) {
          cb(new Error("transform failed"));
        },
      }),
      new PassThrough(),
    );
  } catch (e) {
    console.log("rejected", e.message);
  }

  // finished as a promise.
  const r3 = Readable.from(["q"]);
  r3.resume();
  await sp.finished(r3);
  console.log("finished", r3.readableEnded);

  // Operators (experimental in node, but present).
  console.log("map", await Readable.from([1, 2, 3]).map((x) => x * 2).toArray());
  console.log("filter", await Readable.from([1, 2, 3, 4]).filter((x) => x % 2 === 0).toArray());
  console.log("reduce", await Readable.from([1, 2, 3]).reduce((a, x) => a + x, 0));
  console.log("take/drop", await Readable.from([1, 2, 3, 4, 5]).drop(1).take(2).toArray());
  console.log("some", await Readable.from([1, 2]).some((x) => x > 1), await Readable.from([1, 2]).every((x) => x > 1));
  console.log("find", await Readable.from([5, 6, 7]).find((x) => x > 5));
  const seen = [];
  await Readable.from(["f", "g"]).forEach((x) => seen.push(x));
  console.log("forEach", seen);
  console.log("flatMap", await Readable.from([1, 2]).flatMap((x) => [x, x]).toArray());

  // duplexPair: what one side writes, the other reads.
  const [a, b] = duplexPair();
  a.write("ping");
  b.once("data", (d) => {
    console.log("b got", String(d));
    b.end("pong");
  });
  a.on("data", (d) => console.log("a got", String(d)));
  await new Promise((resolve) => finished(a, { writable: false }, resolve));
  a.end();
  console.log("pair done");

  // Readable.from a string and a Buffer: one chunk each.
  console.log(await Readable.from("whole").toArray(), (await Readable.from(Buffer.from("buf")).toArray()).map(String));
}
main().then(() => console.log("main done"));
