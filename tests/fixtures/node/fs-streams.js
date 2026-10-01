// fs.createReadStream / createWriteStream (T60): node 24's fs streams over T59's fs.
import fs from "node:fs";
import { pipeline } from "node:stream/promises";
import { Transform } from "node:stream";

const ev = (name, s) => {
  for (const e of ["open", "ready", "end", "finish", "close"]) s.on(e, (x) => console.log(name, e, x === undefined ? "" : typeof x));
};

async function main() {
  // Chunks of highWaterMark bytes, with an encoding.
  const r = fs.createReadStream("lines.txt", { highWaterMark: 8, encoding: "utf8" });
  ev("r", r);
  const parts = [];
  for await (const c of r) parts.push(c);
  console.log(parts, r.bytesRead, r.path, typeof r.fd, r.pending);

  // start/end (inclusive).
  const part = [];
  for await (const c of fs.createReadStream("lines.txt", { start: 5, end: 11 })) part.push(String(c));
  console.log("range", JSON.stringify(part.join("")));

  // A big file: chunk count and checksum.
  let n = 0;
  let sum = 0;
  let chunks = 0;
  for await (const c of fs.createReadStream("big.bin")) {
    chunks++;
    n += c.length;
    for (const b of c) sum = (sum + b) % 65521;
  }
  console.log("big", chunks, n, sum);

  // Write, then append.
  const w = fs.createWriteStream("out.txt");
  ev("w", w);
  w.write("first\n");
  w.write(Buffer.from("second\n"));
  await new Promise((resolve) => w.end("third\n", resolve));
  console.log("bytesWritten", w.bytesWritten, JSON.stringify(fs.readFileSync("out.txt", "utf8")));
  const a = fs.createWriteStream("out.txt", { flags: "a" });
  a.end("appended\n");
  await new Promise((resolve) => a.on("close", resolve));
  console.log(JSON.stringify(fs.readFileSync("out.txt", "utf8")));

  // Copy through a transform with stream/promises' pipeline.
  await pipeline(
    fs.createReadStream("lines.txt"),
    new Transform({
      transform(c, e, cb) {
        cb(null, String(c).toUpperCase());
      },
    }),
    fs.createWriteStream("upper.txt"),
  );
  console.log(JSON.stringify(fs.readFileSync("upper.txt", "utf8")));

  // pipe() copy of the big file.
  await new Promise((resolve) => fs.createReadStream("big.bin").pipe(fs.createWriteStream("copy.bin")).on("finish", resolve));
  console.log("copy equal", Buffer.compare(fs.readFileSync("big.bin"), fs.readFileSync("copy.bin")) === 0);

  // Errors: a missing file, a directory.
  const missing = fs.createReadStream("nope.txt");
  await new Promise((resolve) =>
    missing.on("error", (e) => {
      console.log("error", e.code, e.syscall, e.message);
      resolve();
    }),
  );
  console.log("destroyed", missing.destroyed);
  try {
    fs.createReadStream("lines.txt", { start: 5, end: 2 });
  } catch (e) {
    console.log(e.name, e.code, e.message);
  }
  console.log(typeof fs.ReadStream, fs.ReadStream === fs.FileReadStream, fs.createReadStream("lines.txt").destroy() instanceof fs.ReadStream);
}
main().then(() => console.log("done"));
