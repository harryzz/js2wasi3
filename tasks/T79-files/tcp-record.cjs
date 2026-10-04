// T79: a recording TCP proxy. Listens on <listen port> (127.0.0.1), connects each client to <target host:port>,
// and writes every chunk of both directions as JSON lines: {"t": ms since the proxy started (fractional),
// "c": connection number, "d": "up" (client → target) | "down" (target → client) | "open" | "close", "x": hex}.
// Used between zigbee2mqtt and the coordinator (Z-Stack over TCP) and between zigbee2mqtt and the MQTT broker.
//   node tcp-record.cjs <listen port> <target host:port> <out.jsonl>
"use strict";
const net = require("node:net");
const fs = require("node:fs");

const [port, target, outPath] = process.argv.slice(2);
if (!port || !target || !outPath) {
  console.error("usage: node tcp-record.cjs <listen port> <target host:port> <out.jsonl>");
  process.exit(2);
}
const [host, tport] = [target.slice(0, target.lastIndexOf(":")), +target.slice(target.lastIndexOf(":") + 1)];
const out = fs.createWriteStream(outPath, { flags: "w" });
const t0 = process.hrtime.bigint();
const now = () => Number(process.hrtime.bigint() - t0) / 1e6;
const log = (c, d, buf) => out.write(JSON.stringify({ t: +now().toFixed(3), c, d, ...(buf ? { x: buf.toString("hex") } : {}) }) + "\n");
out.write(JSON.stringify({ start: new Date().toISOString(), target }) + "\n");

let n = 0;
net.createServer((client) => {
  const c = ++n;
  client.setNoDelay(true);
  const upstream = net.connect({ host, port: tport }, () => log(c, "open"));
  upstream.setNoDelay(true);
  upstream.setKeepAlive(true, 15000);
  client.on("data", (b) => {
    log(c, "up", b);
    upstream.write(b);
  });
  upstream.on("data", (b) => {
    log(c, "down", b);
    client.write(b);
  });
  const end = (why) => () => {
    log(c, "close:" + why);
    client.destroy();
    upstream.destroy();
  };
  client.on("close", end("client"));
  upstream.on("close", end("target"));
  client.on("error", () => {});
  upstream.on("error", (e) => log(c, "error:" + e.code));
}).listen(+port, "127.0.0.1", () => console.error(`recording 127.0.0.1:${port} -> ${target} into ${outPath}`));

for (const s of ["SIGINT", "SIGTERM"]) process.on(s, () => out.end(() => process.exit(0)));
