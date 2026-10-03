// T70 (F300): `fs.promises` settle in a later turn of the event loop (node: on its thread pool), so
// `setImmediate` callbacks and a destroyed socket's `close` run between awaited file operations.
// diff.sh copies `fs-promises-turn.fs/` and runs in the copy.
import net from "node:net";
import fs from "node:fs";
import { readFile, stat, writeFile } from "node:fs/promises";
import { once } from "node:events";

// A destroyed socket's `close` comes before the next file result (zigbee-herdsman's restart).
const server = net.createServer((s) => s.on("error", () => {}));
server.listen(0, "127.0.0.1");
await once(server, "listening");
const sock = net.connect(server.address().port, "127.0.0.1");
await once(sock, "connect");
sock.once("close", () => console.log("close"));
sock.destroy();
console.log("destroyed");
for (let i = 0; i < 3; i++) {
  await readFile("data.txt", "utf8");
  console.log("readFile", i);
}
server.close();

// A setImmediate scheduled before an awaited stat runs first; a microtask runs before both.
setImmediate(() => console.log("immediate"));
queueMicrotask(() => console.log("microtask"));
const st = await stat("data.txt");
console.log("stat", st.size);

// Rejections settle later too.
setImmediate(() => console.log("immediate 2"));
try {
  await readFile("missing.txt");
} catch (e) {
  console.log("rejected", e.code);
}

// The promise is pending when returned.
let settled = false;
const p = writeFile("out.txt", "x").then(() => { settled = true; });
await Promise.resolve();
console.log("settled after a microtask:", settled);
await p;
console.log("written", fs.readFileSync("out.txt", "utf8"));
